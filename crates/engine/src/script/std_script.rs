//! The `gStdScripts` label table (S-5, slice 2, issue #93).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the *shape* of
//! upstream's `gStdScripts` (`pokeemerald/data/event_scripts.s:96-108`): an
//! 11-entry table `gotostd`/`callstd`/`gotostd_if`/`callstd_if`
//! (`pokeemerald/src/scrcmd.c`'s `ScrCmd_gotostd`/`ScrCmd_callstd`/
//! `ScrCmd_gotostd_if`/`ScrCmd_callstd_if`) index into by a single byte
//! operand. Only the *labels* are transcribed here, in upstream's exact
//! table order — not the scripts themselves (`Std_ObtainItem`,
//! `Std_MsgboxNPC`, …), which are game *content* `(no-verbatim)` out of scope
//! until the message/menu subsystems those scripts drive exist. See
//! [`crate::script::commands`]'s module docs for how the command layer uses
//! this table.

/// One label in upstream's `gStdScripts` array, named and ordered exactly as
/// `data/event_scripts.s:96-108` defines them; the discriminant is the byte
/// `gotostd`/`callstd`/`gotostd_if`/`callstd_if` read to select it, matching
/// each entry's index into `gStdScripts`.
///
/// Four of upstream's eleven entries also have a symbolic `STD_*`/`MSGBOX_*`
/// constant defined alongside the `gotostd`/`callstd` macros
/// (`asm/macros/event.inc:62-65,1924-1930`) that field scripts actually
/// author with; both sets of names are recorded in each variant's doc comment
/// since they name the same slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StdScript {
    /// Index 0. `Std_ObtainItem` / `STD_OBTAIN_ITEM`.
    ObtainItem = 0,
    /// Index 1. `Std_FindItem` / `STD_FIND_ITEM`.
    FindItem = 1,
    /// Index 2. `Std_MsgboxNPC` / `MSGBOX_NPC`.
    MsgboxNpc = 2,
    /// Index 3. `Std_MsgboxSign` / `MSGBOX_SIGN`.
    MsgboxSign = 3,
    /// Index 4. `Std_MsgboxDefault` / `MSGBOX_DEFAULT`.
    MsgboxDefault = 4,
    /// Index 5. `Std_MsgboxYesNo` / `MSGBOX_YESNO`.
    MsgboxYesNo = 5,
    /// Index 6. `Std_MsgboxAutoclose` / `MSGBOX_AUTOCLOSE`.
    MsgboxAutoclose = 6,
    /// Index 7. `Std_ObtainDecoration` / `STD_OBTAIN_DECORATION`.
    ObtainDecoration = 7,
    /// Index 8. `Std_RegisteredInMatchCall` / `STD_REGISTER_MATCH_CALL`.
    RegisteredInMatchCall = 8,
    /// Index 9. `Std_MsgboxGetPoints` / `MSGBOX_GETPOINTS`.
    MsgboxGetPoints = 9,
    /// Index 10. `Std_MsgboxPokenav` / `MSGBOX_POKENAV`.
    MsgboxPokenav = 10,
}

impl StdScript {
    /// The length of upstream's `gStdScripts` array (`gStdScripts_End -
    /// gStdScripts`).
    pub const COUNT: u8 = 11;

    /// Classify a raw `gotostd`/`callstd`/`gotostd_if`/`callstd_if` index
    /// operand, mirroring upstream's `ptr < gStdScripts_End` bounds check
    /// (`src/scrcmd.c`) — `None` for an index that check would reject.
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

    /// This label's index into `gStdScripts` — the inverse of
    /// [`from_index`](Self::from_index).
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
        let expected = [
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
        assert_eq!(expected.len(), usize::from(StdScript::COUNT));
        for (i, &want) in expected.iter().enumerate() {
            let i = u8::try_from(i).unwrap();
            assert_eq!(StdScript::from_index(i), Some(want), "index {i}");
            assert_eq!(want.index(), i);
        }
    }

    #[test]
    fn from_index_rejects_indices_past_gstdscripts_end() {
        assert_eq!(StdScript::from_index(StdScript::COUNT), None);
        assert_eq!(StdScript::from_index(u8::MAX), None);
    }

    // Ground-truth check, transcribed independently from
    // `asm/macros/event.inc:62-65,1924-1930`, not derived from this module's
    // code: the symbolic STD_*/MSGBOX_* constants field scripts actually
    // author with must land on the same indices `gStdScripts`' declaration
    // order gives them.
    #[test]
    fn named_constants_match_upstream_macro_definitions() {
        assert_eq!(StdScript::ObtainItem.index(), 0); // STD_OBTAIN_ITEM
        assert_eq!(StdScript::FindItem.index(), 1); // STD_FIND_ITEM
        assert_eq!(StdScript::ObtainDecoration.index(), 7); // STD_OBTAIN_DECORATION
        assert_eq!(StdScript::RegisteredInMatchCall.index(), 8); // STD_REGISTER_MATCH_CALL
        assert_eq!(StdScript::MsgboxNpc.index(), 2); // MSGBOX_NPC
        assert_eq!(StdScript::MsgboxSign.index(), 3); // MSGBOX_SIGN
        assert_eq!(StdScript::MsgboxDefault.index(), 4); // MSGBOX_DEFAULT
        assert_eq!(StdScript::MsgboxYesNo.index(), 5); // MSGBOX_YESNO
        assert_eq!(StdScript::MsgboxAutoclose.index(), 6); // MSGBOX_AUTOCLOSE
        assert_eq!(StdScript::MsgboxGetPoints.index(), 9); // MSGBOX_GETPOINTS
        assert_eq!(StdScript::MsgboxPokenav.index(), 10); // MSGBOX_POKENAV
    }
}
