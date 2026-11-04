import { readFileSync } from 'node:fs'
import { REPO_URL } from '../consts'

const REPO = 'niraj-khatiwada/enso'

/** The latest release's page, which always offers the newest packages */
export const LATEST_RELEASE_URL = `${REPO_URL}/releases/latest`

interface GitHubRelease {
  tag_name: string
  html_url: string
  assets: Array<{ name: string; browser_download_url: string; size: number }>
}

export interface Download {
  /** The architecture, when the row offers more than one: x64 or ARM64 */
  label: string
  file: string
  href: string
  /** Size in MB, when the release reports it */
  size?: number
}

/** One kind of package (a disk image, an installer, a .deb…), in each architecture built */
export interface Row {
  label: string
  /** The file extension, shown beside the label */
  ext: string
  files: Download[]
}

export interface Platform {
  id: 'macos' | 'windows' | 'linux'
  name: string
  note: string
  rows: Row[]
}

/** The app's version, read from its Cargo.toml whenever the site is built */
export function appVersion(): string {
  try {
    const toml = readFileSync(new URL('../../../Cargo.toml', import.meta.url), 'utf8')
    return toml.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? '1.0.0'
  } catch {
    return '1.0.0'
  }
}

/**
 * The published release of `version`, or `undefined` when it isn't published yet, GitHub can't be
 * reached or it refuses (its hourly limit for anonymous requests), so the site still builds.
 */
const cache = new Map<string, Promise<GitHubRelease | undefined>>()

function releaseOf(version: string): Promise<GitHubRelease | undefined> {
  // One request per build (or dev server run), so reloading never exhausts GitHub's limit
  if (!cache.has(version)) {
    cache.set(
      version,
      fetchRelease(version).then((release) => {
        if (!release) cache.delete(version) // try again next time
        return release
      })
    )
  }
  return cache.get(version)!
}

async function fetchRelease(version: string): Promise<GitHubRelease | undefined> {
  try {
    const token = process.env.GITHUB_TOKEN
    const response = await fetch(`https://api.github.com/repos/${REPO}/releases/tags/v${version}`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    })
    if (response.ok) return (await response.json()) as GitHubRelease
    console.warn(`GitHub has no release v${version} (${response.status}); download links use the releases page.`)
  } catch (error) {
    console.warn('GitHub API could not be reached; download links use the releases page.', error)
  }
  return undefined
}

/**
 * Every package `.github/workflows/release.yml` builds for the app's version (from Cargo.toml),
 * linked to its release's files once that release is published, else to the releases page
 */
export async function getPlatforms(): Promise<{ version: string; releaseUrl: string; platforms: Platform[] }> {
  const version = appVersion()
  const release = await releaseOf(version)

  const asset = (label: string, file: string): Download => {
    const found = release?.assets.find((a) => a.name === file)
    return {
      label,
      file,
      href: found?.browser_download_url ?? LATEST_RELEASE_URL,
      size: found ? Math.round(found.size / 1024 / 1024) : undefined,
    }
  }

  return {
    version,
    releaseUrl: release?.html_url ?? LATEST_RELEASE_URL,
    platforms: [
      {
        id: 'macos',
        name: 'macOS',
        note: 'Includes Quick Look previews in Finder.',
        rows: [
          { label: 'Apple Silicon', ext: '.dmg', files: [asset('Apple Silicon', `enso-${version}-arm64.dmg`)] },
          { label: 'Intel', ext: '.dmg', files: [asset('Intel', `enso-${version}-x64.dmg`)] },
        ],
      },
      {
        id: 'windows',
        name: 'Windows',
        note: 'The installer adds enso to the Start menu. The zip runs from any folder.',
        rows: [
          {
            label: 'Installer',
            ext: '.exe',
            files: [
              asset('x64', `enso-${version}-windows-x64-setup.exe`),
              asset('ARM64', `enso-${version}-windows-arm64-setup.exe`),
            ],
          },
          {
            label: 'Portable',
            ext: '.zip',
            files: [
              asset('x64', `enso-${version}-windows-x64.zip`),
              asset('ARM64', `enso-${version}-windows-arm64.zip`),
            ],
          },
        ],
      },
      {
        id: 'linux',
        name: 'Linux',
        note: 'AppImage runs on most distros, .deb on Debian and Ubuntu, .rpm on Fedora and openSUSE.',
        rows: (
          [
            ['AppImage', '.AppImage'],
            ['Debian', '.deb'],
            ['Fedora', '.rpm'],
            ['Archive', '.tar.gz'],
          ] as const
        ).map(([label, ext]) => ({
          label,
          ext,
          files: [asset('x64', `enso-${version}-linux-x64${ext}`), asset('ARM64', `enso-${version}-linux-arm64${ext}`)],
        })),
      },
    ],
  }
}
