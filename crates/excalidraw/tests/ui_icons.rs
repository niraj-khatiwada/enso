use std::collections::BTreeSet;

use gpui_excalidraw::ui::icons::{IconAssets, has_icon, icon_names, icon_path, icon_source};
use gpui_kit::AssetSource;

const REQUIRED: &[&str] = &[
    "LockedIcon",
    "UnlockedIcon",
    "handIcon",
    "SelectionIcon",
    "RectangleIcon",
    "DiamondIcon",
    "EllipseIcon",
    "ArrowIcon",
    "LineIcon",
    "FreedrawIcon",
    "TextIcon",
    "stickyNoteToolIcon",
    "EraserIcon",
    "ImageIcon",
    "frameToolIcon",
    "laserPointerToolIcon",
    "bucketFillIcon",
    "EmbedIcon",
    "drawShapeToolIcon",
    "HamburgerMenuIcon",
    "sidebarRightIcon",
    "LibraryIcon",
    "MoonIcon",
    "SunIcon",
    "ZoomInIcon",
    "ZoomOutIcon",
    "UndoIcon",
    "RedoIcon",
    "DotsIcon",
    "LoadIcon",
    "ExportIcon",
    "ExportImageIcon",
    "exportToFileIcon",
    "downloadIcon",
    "TrashIcon",
    "HelpIcon",
    "PlusIcon",
    "clipboard",
    "BringForwardIcon",
    "SendBackwardIcon",
    "BringToFrontIcon",
    "SendToBackIcon",
    "DuplicateIcon",
    "GroupIcon",
    "UngroupIcon",
    "LinkIcon",
    "selectAllIcon",
    "copyIcon",
    "cutIcon",
    "eyeIcon",
    "eyeClosedIcon",
    "AlignLeftIcon",
    "CenterHorizontallyIcon",
    "AlignRightIcon",
    "AlignTopIcon",
    "CenterVerticallyIcon",
    "AlignBottomIcon",
    "DistributeHorizontallyIcon",
    "DistributeVerticallyIcon",
    "FillHachureIcon",
    "FillCrossHatchIcon",
    "FillSolidIcon",
    "FillZigZagIcon",
    "StrokeStyleSolidIcon",
    "StrokeStyleDashedIcon",
    "StrokeStyleDottedIcon",
    "StrokeWidthBaseIcon",
    "StrokeWidthBoldIcon",
    "StrokeWidthExtraBoldIcon",
    "SloppinessArchitectIcon",
    "SloppinessArtistIcon",
    "SloppinessCartoonistIcon",
    "EdgeSharpIcon",
    "EdgeRoundIcon",
    "FontFamilyNormalIcon",
    "FontFamilyHeadingIcon",
    "FontFamilyCodeIcon",
    "FontSizeSmallIcon",
    "FontSizeMediumIcon",
    "FontSizeLargeIcon",
    "FontSizeExtraLargeIcon",
    "TextAlignLeftIcon",
    "TextAlignCenterIcon",
    "TextAlignRightIcon",
    "ArrowheadNoneIcon",
    "ArrowheadArrowIcon",
    "ArrowheadTriangleIcon",
    "ArrowheadCircleIcon",
    "ArrowheadDiamondIcon",
    "ArrowheadBarIcon",
    "palette",
    "strokeIcon",
    "gridIcon",
    "magnetIcon",
    "zoomAreaIcon",
];

#[test]
fn every_required_icon_is_bundled() {
    let missing: Vec<&str> = REQUIRED
        .iter()
        .copied()
        .filter(|name| !has_icon(name))
        .collect();
    assert!(missing.is_empty(), "missing bundled icons: {missing:?}");
}

#[test]
fn bundled_icons_are_well_formed_svg() {
    let names: Vec<&str> = icon_names().collect();
    assert!(
        names.len() >= 90,
        "expected the full toolbar/panel icon set, found {}",
        names.len()
    );

    for name in names {
        let svg = icon_source(name).expect("name came from the icon table");
        assert!(svg.starts_with("<svg"), "{name} is not an SVG document");
        assert!(
            svg.trim_end().ends_with("</svg>"),
            "{name} is truncated: {}",
            &svg[svg.len().saturating_sub(40)..]
        );
        assert!(svg.contains("viewBox="), "{name} has no viewBox");

        for leak in [
            "strokeWidth=",
            "strokeLinecap=",
            "strokeLinejoin=",
            "clipRule=",
            "{{",
            "<></>",
            "iconFillColor",
            "=currentColor",
            "=none",
            "JSX",
        ] {
            assert!(!svg.contains(leak), "{name} still contains {leak:?}");
        }
    }
}

#[test]
fn icon_names_are_unique() {
    let names: Vec<&str> = icon_names().collect();
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(names.len(), unique.len(), "two icons share a file name");
}

#[test]
fn asset_source_resolves_every_icon_and_misses_cleanly() {
    let assets = IconAssets;

    for name in icon_names() {
        let path = icon_path(name);
        assert_eq!(path, format!("icons/{name}.svg"));
        let bytes = assets
            .load(&path)
            .unwrap_or_else(|e| panic!("loading {path} errored: {e}"))
            .unwrap_or_else(|| panic!("{path} did not resolve"));
        assert!(bytes.starts_with(b"<svg"), "{path} did not serve SVG bytes");
    }

    // A path outside the bundled set falls through to the wrapped asset store,
    // which reports a miss as `Err` rather than `Ok(None)`. Either shape is a
    // clean miss; what must never happen is returning SVG bytes for it.
    let miss = assets.load("icons/DefinitelyNotAnIcon.svg");
    assert!(
        matches!(&miss, Ok(None) | Err(_)),
        "an unknown icon must not resolve to bytes: {miss:?}"
    );
    assert!(
        assets
            .load("")
            .expect("empty path is not an error")
            .is_none()
    );

    let listed = assets.list("icons/").expect("list must not error");
    for name in icon_names() {
        let expected = format!("icons/{name}.svg");
        assert!(
            listed.iter().any(|entry| entry.as_ref() == expected),
            "{expected} is missing from the listed assets"
        );
    }
}

#[test]
fn toolbar_and_chrome_icons_resolve_to_non_trivial_markup() {
    for name in [
        "SelectionIcon",
        "RectangleIcon",
        "DiamondIcon",
        "EllipseIcon",
        "ArrowIcon",
        "LineIcon",
        "FreedrawIcon",
        "TextIcon",
        "stickyNoteToolIcon",
        "EraserIcon",
        "handIcon",
        "LockedIcon",
    ] {
        let svg = icon_source(name).expect(name);
        assert!(
            svg.len() > 150,
            "{name} looks like a stub ({} bytes)",
            svg.len()
        );
        assert!(
            svg.matches("<path").count()
                + svg.matches("<rect").count()
                + svg.matches("<circle").count()
                + svg.matches("<line").count()
                > 0,
            "{name} declares no geometry"
        );
    }
}
