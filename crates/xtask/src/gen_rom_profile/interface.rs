//! Locating the interface images and palettes.
//!
//! The one image is the text printer's down-arrow sheet, `sDownArrowTiles`
//! (`src/text.c:71`): raw 4bpp tiles, searched for through [`super::images`]
//! and accepted only at a single address, since a second copy would be
//! evidence the search is wrong rather than a twin to pair. The palettes are
//! located by [`super::locate_interface_palettes`].

use super::error::GenRomProfileError;
use super::images::{locate_unique_images, ImageQuery};
use super::inventory;
use super::plan::{InterfacePlan, ReportLine};
use super::Context;
use crate::extract::scope::INTERFACE_IMAGE_IDS;

/// Locate every interface image and palette.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] if the pack lacks a required
/// interface image or palette, otherwise any error
/// [`locate_unique_images`] or [`super::locate_interface_palettes`] raises.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<InterfacePlan, GenRomProfileError> {
    inventory::require_interface_images(ctx.pack)?;
    let queries: Vec<ImageQuery> = INTERFACE_IMAGE_IDS
        .into_iter()
        .map(ImageQuery::raw)
        .collect();
    Ok(InterfacePlan {
        images: locate_unique_images(ctx, &queries, report)?,
        palettes: super::locate_interface_palettes(ctx, report)?,
    })
}

#[cfg(test)]
#[path = "interface_tests.rs"]
mod tests;
