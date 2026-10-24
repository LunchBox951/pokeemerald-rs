use super::super::StagingArea;
use crate::save::file::SaveFile;

pub(super) fn area(file: &SaveFile) -> StagingArea<'_> {
    StagingArea::beside(file.path())
}
