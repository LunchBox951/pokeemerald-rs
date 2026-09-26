use super::super::StagingArea;
use crate::save::file::SaveFile;

/// The staging area beside `file`'s save path.
pub(super) fn area(file: &SaveFile) -> StagingArea<'_> {
    StagingArea::beside(file.path())
}
