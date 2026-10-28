// Enso's Quick Look preview for Markdown: the file rendered to HTML by the Rust
// renderer (bridge.h), styled like Enso's reading view. Local images are read
// from beside the file and attached, since a preview cannot load file URLs.

import Foundation
import QuickLookUI
import UniformTypeIdentifiers

@objc(EnsoPreviewProvider)
final class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    func providePreview(for request: QLFilePreviewRequest) async throws -> QLPreviewReply {
        let url = request.fileURL
        let data = try Data(contentsOf: url)
        let source = String(decoding: data, as: UTF8.self)
        let rendered = source.withCString { pointer -> String in
            guard let html = enso_quicklook_render(pointer) else { return "" }
            defer { enso_quicklook_free(html) }
            return String(cString: html)
        }
        var attachments: [String: QLPreviewReplyAttachment] = [:]
        let body = attachImages(in: rendered, folder: url.deletingLastPathComponent(), into: &attachments)
        let page = document(title: url.deletingPathExtension().lastPathComponent, body: body)
        let reply = QLPreviewReply(dataOfContentType: .html, contentSize: CGSize(width: 860, height: 1000)) { reply in
            reply.stringEncoding = .utf8
            reply.attachments = attachments
            return Data(page.utf8)
        }
        reply.title = url.lastPathComponent
        return reply
    }
}

/// Point every `<img src>` that names a local file at an attachment carrying
/// that file's bytes. Remote and unreadable images are left as they are.
private func attachImages(
    in html: String,
    folder: URL,
    into attachments: inout [String: QLPreviewReplyAttachment]
) -> String {
    guard let pattern = try? NSRegularExpression(pattern: #"(<img\b[^>]*?\bsrc=")([^"]*)(")"#, options: [.caseInsensitive]) else {
        return html
    }
    let source = html as NSString
    var output = ""
    var cursor = 0
    for match in pattern.matches(in: html, range: NSRange(location: 0, length: source.length)) {
        let srcRange = match.range(at: 2)
        output += source.substring(with: NSRange(location: cursor, length: srcRange.location - cursor))
        let src = source.substring(with: srcRange)
        output += attachment(for: src, folder: folder, into: &attachments) ?? src
        cursor = srcRange.location + srcRange.length
    }
    output += source.substring(from: cursor)
    return output
}

private func attachment(
    for src: String,
    folder: URL,
    into attachments: inout [String: QLPreviewReplyAttachment]
) -> String? {
    let decoded = (src.replacingOccurrences(of: "&amp;", with: "&").removingPercentEncoding) ?? src
    if decoded.contains("://") || decoded.hasPrefix("data:") || decoded.hasPrefix("cid:") {
        return nil
    }
    let file = decoded.hasPrefix("/")
        ? URL(fileURLWithPath: decoded)
        : folder.appendingPathComponent(decoded).standardizedFileURL
    guard let data = try? Data(contentsOf: file) else { return nil }
    let type = UTType(filenameExtension: file.pathExtension) ?? .image
    let name = "image\(attachments.count)"
    attachments[name] = QLPreviewReplyAttachment(data: data, contentType: type)
    return "cid:\(name)"
}

private func document(title: String, body: String) -> String {
    let escapedTitle = title
        .replacingOccurrences(of: "&", with: "&amp;")
        .replacingOccurrences(of: "<", with: "&lt;")
    return """
    <!DOCTYPE html>
    <html>
    <head>
    <meta charset="utf-8">
    <meta name="color-scheme" content="light dark">
    <title>\(escapedTitle)</title>
    <style>
    :root {
      --text: #1f2328; --muted: #656d76; --border: #d8dee4; --surface: #f6f8fa;
      --link: #0969da; --background: #ffffff;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --text: #e6e6e6; --muted: #9a9a9a; --border: #333333; --surface: #1c1c1c;
        --link: #6cb6ff; --background: #121212;
      }
    }
    html { background: var(--background); }
    body {
      max-width: 760px; margin: 0 auto; padding: 40px 48px 64px;
      font: 15px/1.65 -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif;
      color: var(--text); -webkit-font-smoothing: antialiased; word-wrap: break-word;
    }
    h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.6em 0 0.6em; font-weight: 600; }
    h1 { font-size: 2em; } h2 { font-size: 1.5em; } h3 { font-size: 1.25em; }
    h1:first-child, h2:first-child, h3:first-child { margin-top: 0; }
    p, ul, ol, blockquote, pre, table { margin: 0 0 1em; }
    a { color: var(--link); text-decoration: none; }
    img { max-width: 100%; height: auto; border-radius: 6px; }
    hr { border: 0; border-top: 1px solid var(--border); margin: 2em 0; }
    blockquote { margin-left: 0; padding: 0 1em; color: var(--muted); border-left: 3px solid var(--border); }
    code, pre { font: 13px/1.5 ui-monospace, "SF Mono", Menlo, monospace; }
    code { background: var(--surface); padding: 0.15em 0.35em; border-radius: 4px; }
    pre { background: var(--surface); padding: 12px 14px; border-radius: 8px; overflow-x: auto; }
    pre code { background: none; padding: 0; }
    table { border-collapse: collapse; display: block; overflow-x: auto; }
    th, td { border: 1px solid var(--border); padding: 6px 12px; }
    th { background: var(--surface); font-weight: 600; }
    li > input[type=checkbox] { margin: 0 0.4em 0 -1.3em; }
    ul:has(> li > input[type=checkbox]) { list-style: none; }
    </style>
    </head>
    <body>
    \(body)
    </body>
    </html>
    """
}
