//! Standard event-script identifiers.
//!
//! The discriminants preserve `gStdScripts` table order
//! (`data/event_scripts.s:96-108`) because `gotostd` and `callstd` bytecode
//! encode the table index as one byte.

/// A `gStdScripts` entry whose discriminant is its encoded table index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StdScript {
    /// `Std_ObtainItem`, also encoded as `STD_OBTAIN_ITEM`.
    ObtainItem = 0,
    /// `Std_FindItem`, also encoded as `STD_FIND_ITEM`.
    FindItem = 1,
    /// `Std_MsgboxNPC`, also encoded as `MSGBOX_NPC`.
    MsgboxNpc = 2,
    /// `Std_MsgboxSign`, also encoded as `MSGBOX_SIGN`.
    MsgboxSign = 3,
    /// `Std_MsgboxDefault`, also encoded as `MSGBOX_DEFAULT`.
    MsgboxDefault = 4,
    /// `Std_MsgboxYesNo`, also encoded as `MSGBOX_YESNO`.
    MsgboxYesNo = 5,
    /// `Std_MsgboxAutoclose`, also encoded as `MSGBOX_AUTOCLOSE`.
    MsgboxAutoclose = 6,
    /// `Std_ObtainDecoration`, also encoded as `STD_OBTAIN_DECORATION`.
    ObtainDecoration = 7,
    /// `Std_RegisteredInMatchCall`, also encoded as `STD_REGISTER_MATCH_CALL`.
    RegisteredInMatchCall = 8,
    /// `Std_MsgboxGetPoints`, also encoded as `MSGBOX_GETPOINTS`.
    MsgboxGetPoints = 9,
    /// `Std_MsgboxPokenav`, also encoded as `MSGBOX_POKENAV`.
    MsgboxPokenav = 10,
}

impl StdScript {
    /// Number of valid standard-script indices.
    pub const COUNT: u8 = 11;

    /// Validates a standard-script index read from bytecode.
    ///
    /// Upstream ignores out-of-range indices (`src/scrcmd.c:236-284`), so
    /// callers use `None` as a no-op.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::ObtainItem),
            1 => Some(Self::FindItem),
            2 => Some(Self::MsgboxNpc),
            3 => Some(Self::MsgboxSign),
            4 => Some(Self::MsgboxDefault),
            5 => Some(Self::MsgboxYesNo),
            6 => Some(Self::MsgboxAutoclose),
            7 => Some(Self::ObtainDecoration),
            8 => Some(Self::RegisteredInMatchCall),
            9 => Some(Self::MsgboxGetPoints),
            10 => Some(Self::MsgboxPokenav),
            _ => None,
        }
    }

    /// Returns this entry's encoded table index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_index_covers_every_table_slot_in_order() {
        let expected_order = [
            StdScript::ObtainItem,
            StdScript::FindItem,
            StdScript::MsgboxNpc,
            StdScript::MsgboxSign,
            StdScript::MsgboxDefault,
            StdScript::MsgboxYesNo,
            StdScript::MsgboxAutoclose,
            StdScript::ObtainDecoration,
            StdScript::RegisteredInMatchCall,
            StdScript::MsgboxGetPoints,
            StdScript::MsgboxPokenav,
        ];
        assert_eq!(expected_order.len(), usize::from(StdScript::COUNT));
        for (index, &expected_script) in expected_order.iter().enumerate() {
            let index = u8::try_from(index).unwrap();
            assert_eq!(
                StdScript::from_index(index),
                Some(expected_script),
                "index {index}"
            );
            assert_eq!(expected_script.index(), index);
        }
    }

    #[test]
    fn from_index_rejects_out_of_range_indices() {
        assert_eq!(StdScript::from_index(StdScript::COUNT), None);
        assert_eq!(StdScript::from_index(u8::MAX), None);
    }

    #[test]
    fn encoded_indices_match_upstream_constants() {
        assert_eq!(StdScript::ObtainItem.index(), 0);
        assert_eq!(StdScript::FindItem.index(), 1);
        assert_eq!(StdScript::ObtainDecoration.index(), 7);
        assert_eq!(StdScript::RegisteredInMatchCall.index(), 8);
        assert_eq!(StdScript::MsgboxNpc.index(), 2);
        assert_eq!(StdScript::MsgboxSign.index(), 3);
        assert_eq!(StdScript::MsgboxDefault.index(), 4);
        assert_eq!(StdScript::MsgboxYesNo.index(), 5);
        assert_eq!(StdScript::MsgboxAutoclose.index(), 6);
        assert_eq!(StdScript::MsgboxGetPoints.index(), 9);
        assert_eq!(StdScript::MsgboxPokenav.index(), 10);
    }
}
