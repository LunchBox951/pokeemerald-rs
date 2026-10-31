//! Locating the field-message token table (`src/strings.c`).
//!
//! The pack's `field-message/table` entry is the expectation: the fixed
//! [`FieldMessage`] inventory, each an `EOS`-terminated charmap token stream.
//! The ROM stores each as a bare string. Nothing here records a stream's
//! bytes; only addresses leave this module.
//!
//! A message is found where its stream begins a string (the byte before it is
//! an `EOS`, or it starts the image) and exactly one such place holds it. A
//! message that appears in several places is refused rather than guessed at.
//! The importer's own reader then runs over the finished plan and must yield
//! the pack's payload byte for byte.

use assets::pack::{FieldMessage, FieldMessageTable};

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::exactly_one;
use super::plan::{FieldMessagePlan, FieldMessageRootPlan, ReportLine, Resolution};
use super::Context;

const PACK_ID: &str = rom_import::FIELD_MESSAGE_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "field-message/";
/// The end-of-string byte every stream carries.
const EOS: u8 = 0xFF;

/// Locate the field-message roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `field-message/` but the one table, or the ROM's messages differ from the
/// pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table; [`GenRomProfileError::NotFound`] or
/// [`GenRomProfileError::Ambiguous`] for a message the ROM does not hold
/// exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<FieldMessagePlan, GenRomProfileError> {
    inventory::require_field_messages(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected =
        FieldMessageTable::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
            id: PACK_ID.to_owned(),
            reason: err.to_string(),
        })?;

    let mut messages = Vec::with_capacity(FieldMessage::ALL.len());
    for message in FieldMessage::ALL {
        let stream = expected.string(message);
        let hits: Vec<u32> = ctx
            .raw
            .find_all(&[stream.to_vec()])
            .into_iter()
            .flatten()
            .filter(|&at| starts_string(ctx.rom, at))
            .collect();
        let addr = exactly_one(message.symbol(), &hits)?;
        let len = u32::try_from(stream.len()).expect("a stream fits in u32");
        report.push(
            ReportLine::unique(PACK_ID, addr, len)
                .with(Resolution::UniqueSignature)
                .symbol(message.symbol()),
        );
        messages.push(FieldMessageRootPlan {
            symbol: message.symbol(),
            addr,
        });
    }
    let plan = FieldMessagePlan { messages };
    check_plan(ctx, &plan, payload)?;
    Ok(plan)
}

/// Whether the byte before ROM offset `at` is an `EOS`, or `at` is the start.
fn starts_string(rom: &[u8], at: u32) -> bool {
    let at = usize::try_from(at).expect("a ROM offset fits in usize");
    at == 0 || rom.get(at - 1) == Some(&EOS)
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &FieldMessagePlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let roots: Vec<rom_import::FieldMessageRoot> = plan
        .messages
        .iter()
        .map(|message| {
            let addr = rom_import::GbaPtr::new(message.addr).ok_or_else(|| {
                GenRomProfileError::StructMismatch {
                    id: message.symbol.to_owned(),
                    reason: format!("{:08X} is outside the ROM", message.addr),
                }
            })?;
            Ok(rom_import::FieldMessageRoot {
                symbol: message.symbol,
                addr,
            })
        })
        .collect::<Result<_, GenRomProfileError>>()?;
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded = rom_import::decode_field_message_payload(&reader, &roots).map_err(|err| {
        GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: format!("the ROM's messages do not decode: {err}"),
        }
    })?;
    if decoded != payload {
        return Err(GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: "the ROM's decoded messages differ from the pack entry".to_owned(),
        });
    }
    Ok(())
}
