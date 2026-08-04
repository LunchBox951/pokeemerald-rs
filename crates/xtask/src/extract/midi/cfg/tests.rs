use super::{parse_entry_for, MidiCfgEntry};
use crate::extract::midi::error::MidiError;

const SAMPLE_CFG: &str = "\
mus_abandoned_ship.mid:        -E -R50 -G_abandoned_ship -V080\n\
mus_title.mid:                 -E -R50 -G_title -V090\n\
mus_awaken_legend.mid:         -E -R50 -G_fanfare -V090 -P5\n\
mus_b_dome_lobby.mid:          -E -R50 -G_b_dome -V056\n\
";

#[test]
fn mus_title_entry_matches_the_real_checkout_line() {
    let entry = parse_entry_for(SAMPLE_CFG, "mus_title.mid").unwrap();
    assert_eq!(
        entry,
        MidiCfgEntry {
            voicegroup_label: "title".to_owned(),
            priority: 0,
            reverb: Some(50),
            master_volume: 90,
            exact_gate_time: true,
            clocks_per_beat: 1,
        }
    );
}

#[test]
fn priority_flag_is_parsed() {
    let entry = parse_entry_for(SAMPLE_CFG, "mus_awaken_legend.mid").unwrap();
    assert_eq!(entry.priority, 5);
    assert_eq!(entry.voicegroup_label, "fanfare");
}

#[test]
fn leading_zeros_parse_as_decimal_not_octal() {
    // `-V090` must be 90, matching `std::stoi`'s base-10 default (mid2agb
    // never passes an explicit base, so no `0`-prefix octal interpretation
    // applies) -- see the module docs.
    let entry = parse_entry_for(SAMPLE_CFG, "mus_b_dome_lobby.mid").unwrap();
    assert_eq!(entry.master_volume, 56);
}

#[test]
fn missing_entry_is_reported() {
    let err = parse_entry_for(SAMPLE_CFG, "mus_nonexistent.mid").unwrap_err();
    assert_eq!(
        err,
        MidiError::CfgEntryMissing("mus_nonexistent.mid".to_owned())
    );
}

#[test]
fn x_flag_sets_two_clocks_per_beat() {
    let entry = parse_entry_for("mus_x.mid: -E -G_x -X\n", "mus_x.mid").unwrap();
    assert_eq!(entry.clocks_per_beat, 2);
}

#[test]
fn n_and_l_flags_are_accepted_but_not_stored() {
    let entry = parse_entry_for("mus_n.mid: -E -G_n -N -Lcustom_label\n", "mus_n.mid").unwrap();
    assert_eq!(entry.voicegroup_label, "n");
}

#[test]
fn missing_voicegroup_flag_is_an_error() {
    let err = parse_entry_for("mus_no_g.mid: -E -R50\n", "mus_no_g.mid").unwrap_err();
    assert_eq!(err, MidiError::CfgMissingVoiceGroup);
}

#[test]
fn unrecognized_flag_letter_is_an_error() {
    let err = parse_entry_for("mus_bad.mid: -E -Q5 -G_bad\n", "mus_bad.mid").unwrap_err();
    assert_eq!(err, MidiError::CfgMalformedFlag("-Q5".to_owned()));
}

#[test]
fn non_numeric_operand_is_an_error() {
    let err = parse_entry_for("mus_bad.mid: -E -Rxx -G_bad\n", "mus_bad.mid").unwrap_err();
    assert_eq!(err, MidiError::CfgMalformedFlag("-Rxx".to_owned()));
}

#[test]
fn default_reverb_is_none_and_default_master_volume_is_127() {
    let entry = parse_entry_for("mus_defaults.mid: -E -G_defaults\n", "mus_defaults.mid").unwrap();
    assert_eq!(entry.reverb, None);
    assert_eq!(entry.master_volume, 127);
}
