//! The English staff-roll text: every page of five centred lines, with the
//! title-versus-name styling flag of each line.
//!
//! Source: `src/data/credits.h` (`sCreditsEntryPointerTable` and the
//! `sCreditsText_*` strings). The roll's bicycle scene, Pokémon slideshow,
//! timing, and graphics (`src/credits.c`) are not part of this table.

use std::error::Error;
use std::fmt;

/// The number of credits pages (`PAGE_COUNT`).
pub const CREDITS_PAGE_COUNT: usize = 57;

/// The number of text lines on every page (`ENTRIES_PER_PAGE`).
pub const CREDITS_ENTRIES_PER_PAGE: usize = 5;

/// A failed lookup of a credits page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditsError {
    /// The index is not below [`CREDITS_PAGE_COUNT`].
    UnknownPage(usize),
}

impl fmt::Display for CreditsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPage(index) => write!(f, "unknown credits page `{index}`"),
        }
    }
}

impl Error for CreditsError {}

/// A credits page identifier (`PAGE_*`), in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CreditsPageId {
    /// `PAGE_TITLE`.
    Title,
    /// `PAGE_DIRECTOR`.
    Director,
    /// `PAGE_ART_DIRECTOR`.
    ArtDirector,
    /// `PAGE_WORLD_DIRECTOR`.
    WorldDirector,
    /// `PAGE_LEAD_PROGRAMMER`.
    LeadProgrammer,
    /// `PAGE_PROGRAMMERS_1`.
    Programmers1,
    /// `PAGE_PROGRAMMERS_2`.
    Programmers2,
    /// `PAGE_PROGRAMMERS_3`.
    Programmers3,
    /// `PAGE_PROGRAMMERS_4`.
    Programmers4,
    /// `PAGE_GRAPHIC_DESIGNERS_1`.
    GraphicDesigners1,
    /// `PAGE_GRAPHIC_DESIGNERS_2`.
    GraphicDesigners2,
    /// `PAGE_GRAPHIC_DESIGNERS_3`.
    GraphicDesigners3,
    /// `PAGE_MUSIC_COMPOSITION`.
    MusicComposition,
    /// `PAGE_SOUND_EFFECTS`.
    SoundEffects,
    /// `PAGE_GAME_DESIGNERS_1`.
    GameDesigners1,
    /// `PAGE_GAME_DESIGNERS_2`.
    GameDesigners2,
    /// `PAGE_GAME_DESIGNERS_3`.
    GameDesigners3,
    /// `PAGE_SCENARIO_PLOT`.
    ScenarioPlot,
    /// `PAGE_SCENARIO`.
    Scenario,
    /// `PAGE_SCRIPT_DESIGNERS`.
    ScriptDesigners,
    /// `PAGE_MAP_DESIGNERS`.
    MapDesigners,
    /// `PAGE_BATTLE_FRONTIER_DATA`.
    BattleFrontierData,
    /// `PAGE_PARAMETRIC_DESIGNERS`.
    ParametricDesigners,
    /// `PAGE_POKEDEX_TEXT`.
    PokedexText,
    /// `PAGE_ENVIRONMENT_AND_TOOL_PROGRAMS_1`.
    EnvironmentAndToolPrograms1,
    /// `PAGE_PKMN_DESIGNERS_1`.
    PkmnDesigners1,
    /// `PAGE_PKMN_DESIGNERS_2`.
    PkmnDesigners2,
    /// `PAGE_PKMN_DESIGNERS_3`.
    PkmnDesigners3,
    /// `PAGE_PKMN_DESIGNERS_4`.
    PkmnDesigners4,
    /// `PAGE_SUPPORT_PROGRAMMERS`.
    SupportProgrammers,
    /// `PAGE_NCL_PRODUCT_TESTING`.
    NclProductTesting,
    /// `PAGE_PACKAGE_AND_MANUAL`.
    PackageAndManual,
    /// `PAGE_SPECIAL_THANKS_1`.
    SpecialThanks1,
    /// `PAGE_SPECIAL_THANKS_2`.
    SpecialThanks2,
    /// `PAGE_SPECIAL_THANKS_3`.
    SpecialThanks3,
    /// `PAGE_SPECIAL_THANKS_4`.
    SpecialThanks4,
    /// `PAGE_INFORMATION_SUPERVISORS`.
    InformationSupervisors,
    /// `PAGE_ARTWORK_1`.
    Artwork1,
    /// `PAGE_ARTWORK_2`.
    Artwork2,
    /// `PAGE_ARTWORK_3`.
    Artwork3,
    /// `PAGE_COORDINATORS`.
    Coordinators,
    /// `PAGE_ENGLISH_VERSION`.
    EnglishVersion,
    /// `PAGE_TRANSLATOR`.
    Translator,
    /// `PAGE_TEXT_EDITOR`.
    TextEditor,
    /// `PAGE_NCL_COORDINATOR`.
    NclCoordinator,
    /// `PAGE_PROGRAMMERS_5`.
    Programmers5,
    /// `PAGE_GRAPHIC_DESIGNER`.
    GraphicDesigner,
    /// `PAGE_ENVIRONMENT_AND_TOOL_PROGRAMS_2`.
    EnvironmentAndToolPrograms2,
    /// `PAGE_NOA_TESTING`.
    NoaTesting,
    /// `PAGE_BRAILLE_CODE_CHECK_1`.
    BrailleCodeCheck1,
    /// `PAGE_BRAILLE_CODE_CHECK_2`.
    BrailleCodeCheck2,
    /// `PAGE_SPECIAL_THANKS_5`.
    SpecialThanks5,
    /// `PAGE_TASK_MANAGERS`.
    TaskManagers,
    /// `PAGE_PRODUCERS`.
    Producers,
    /// `PAGE_EXECUTIVE_DIRECTOR`.
    ExecutiveDirector,
    /// `PAGE_EXECUTIVE_PRODUCERS_1`.
    ExecutiveProducers1,
    /// `PAGE_EXECUTIVE_PRODUCERS_2`.
    ExecutiveProducers2,
}

impl CreditsPageId {
    /// Every page identifier in display order.
    pub const ALL: [Self; CREDITS_PAGE_COUNT] = [
        Self::Title,
        Self::Director,
        Self::ArtDirector,
        Self::WorldDirector,
        Self::LeadProgrammer,
        Self::Programmers1,
        Self::Programmers2,
        Self::Programmers3,
        Self::Programmers4,
        Self::GraphicDesigners1,
        Self::GraphicDesigners2,
        Self::GraphicDesigners3,
        Self::MusicComposition,
        Self::SoundEffects,
        Self::GameDesigners1,
        Self::GameDesigners2,
        Self::GameDesigners3,
        Self::ScenarioPlot,
        Self::Scenario,
        Self::ScriptDesigners,
        Self::MapDesigners,
        Self::BattleFrontierData,
        Self::ParametricDesigners,
        Self::PokedexText,
        Self::EnvironmentAndToolPrograms1,
        Self::PkmnDesigners1,
        Self::PkmnDesigners2,
        Self::PkmnDesigners3,
        Self::PkmnDesigners4,
        Self::SupportProgrammers,
        Self::NclProductTesting,
        Self::PackageAndManual,
        Self::SpecialThanks1,
        Self::SpecialThanks2,
        Self::SpecialThanks3,
        Self::SpecialThanks4,
        Self::InformationSupervisors,
        Self::Artwork1,
        Self::Artwork2,
        Self::Artwork3,
        Self::Coordinators,
        Self::EnglishVersion,
        Self::Translator,
        Self::TextEditor,
        Self::NclCoordinator,
        Self::Programmers5,
        Self::GraphicDesigner,
        Self::EnvironmentAndToolPrograms2,
        Self::NoaTesting,
        Self::BrailleCodeCheck1,
        Self::BrailleCodeCheck2,
        Self::SpecialThanks5,
        Self::TaskManagers,
        Self::Producers,
        Self::ExecutiveDirector,
        Self::ExecutiveProducers1,
        Self::ExecutiveProducers2,
    ];

    /// Returns the upstream enum value, which is the page's display position.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// One line of a credits page (`struct CreditsEntry` without its unread
/// leading byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditsEntry {
    /// The line's text; empty for a blank slot (`sCreditsText_EmptyString`).
    pub text: &'static str,
    /// Whether the line is drawn in the title colours rather than the name
    /// colours (`isTitle`).
    pub is_title: bool,
}

/// One credits page: five ordered lines, blank slots included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditsPage {
    /// The page's `PAGE_*` identifier, equal to its table position.
    pub id: CreditsPageId,
    /// The lines in slot order (`sCreditsEntryPointerTable[id]`).
    pub entries: [CreditsEntry; CREDITS_ENTRIES_PER_PAGE],
}

const BLANK: CreditsEntry = CreditsEntry {
    text: "",
    is_title: false,
};

const fn title(text: &'static str) -> CreditsEntry {
    CreditsEntry {
        text,
        is_title: true,
    }
}

const fn name(text: &'static str) -> CreditsEntry {
    CreditsEntry {
        text,
        is_title: false,
    }
}

const fn page(id: CreditsPageId, entries: [CreditsEntry; CREDITS_ENTRIES_PER_PAGE]) -> CreditsPage {
    CreditsPage { id, entries }
}

/// Returns the page at `index` in display order.
///
/// # Errors
///
/// Returns [`CreditsError::UnknownPage`] when `index` is
/// [`CREDITS_PAGE_COUNT`] or more.
pub fn credits_page(index: usize) -> Result<&'static CreditsPage, CreditsError> {
    CREDITS_PAGES
        .get(index)
        .ok_or(CreditsError::UnknownPage(index))
}

/// The number of `sCreditsEntry_*` rows upstream defines that no page uses.
pub const CREDITS_UNUSED_ENTRY_COUNT: usize = 19;

/// Entries defined in `src/data/credits.h` that `sCreditsEntryPointerTable`
/// never references, in declaration order.
pub static CREDITS_UNUSED_ENTRIES: [CreditsEntry; CREDITS_UNUSED_ENTRY_COUNT] = [
    title("Battle Director"),
    title("Main Programmer"),
    title("Battle System Programmers"),
    title("Field System Programmer"),
    title("Main Graphic Designer"),
    title("Map Data Designers"),
    name("Hiromi Sagawa"),
    name("Yoshio Tajiri"),
    name("Teiko Sasaki"),
    name("Sachiko Hamano"),
    name("Chie Matsumiya"),
    name("Akiko Shinozaki"),
    name("Astuko Fujii"),
    name("Atsushi Tada"),
    name("Ai Mashima"),
    name("Takehiro Izushi"),
    name("Kyoko Watanabe"),
    name("Takao Nakano"),
    name("Tomotaka Komura"),
];

/// Every credits page, in `sCreditsEntryPointerTable` order.
pub static CREDITS_PAGES: [CreditsPage; CREDITS_PAGE_COUNT] = [
    page(
        CreditsPageId::Title,
        [
            BLANK,
            title("POKéMON EMERALD VERSION"),
            title("Credits"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Director,
        [
            BLANK,
            title("Director"),
            name("Shigeki Morimoto"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::ArtDirector,
        [
            BLANK,
            title("Art Director"),
            name("Ken Sugimori"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::WorldDirector,
        [
            BLANK,
            title("World Director"),
            name("Junichi Masuda"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::LeadProgrammer,
        [
            title("Lead Programmer"),
            name("Hisashi Sogabe"),
            title("Lead Graphic Artist"),
            name("Motofumi Fujiwara"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Programmers1,
        [
            title("Programmers"),
            name("Hisashi Sogabe"),
            name("Tomomichi Ohta"),
            name("Nozomu Saito"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Programmers2,
        [
            title("Programmers"),
            name("Akito Mori"),
            name("Hiroyuki Nakamura"),
            name("Masao Taya"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Programmers3,
        [
            title("Programmers"),
            name("Satoshi Nohara"),
            name("Miyuki Iwasawa"),
            name("Yoshinori Matsuda"),
            name("Keita Kagaya"),
        ],
    ),
    page(
        CreditsPageId::Programmers4,
        [
            title("Programmers"),
            name("Tetsuya Watanabe"),
            name("Sosuke Tamada"),
            name("Takenori Ohta"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::GraphicDesigners1,
        [
            BLANK,
            title("Graphic Designers"),
            name("Motofumi Fujiwara"),
            name("Satoshi Ohta"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::GraphicDesigners2,
        [
            title("Graphic Designers"),
            name("Kenkichi Toyama"),
            name("Asuka Iwashita"),
            name("Takao Unno"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::GraphicDesigners3,
        [
            title("Graphic Designers"),
            name("Ken Sugimori"),
            name("Hironobu Yoshida"),
            name("Aimi Tomita"),
            name("Kanako Eo"),
        ],
    ),
    page(
        CreditsPageId::MusicComposition,
        [
            title("Music Composition"),
            name("Go Ichinose"),
            name("Junichi Masuda"),
            name("Morikazu Aoki"),
            name("Hitomi Sato"),
        ],
    ),
    page(
        CreditsPageId::SoundEffects,
        [
            BLANK,
            title("Sound Effects & POKéMON Voices"),
            name("Go Ichinose"),
            name("Morikazu Aoki"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::GameDesigners1,
        [
            title("Game Designers"),
            name("Shigeki Morimoto"),
            name("Teruyuki Shimoyamada"),
            name("Takeshi Kawachimaru"),
            name("Akihito Tomisawa"),
        ],
    ),
    page(
        CreditsPageId::GameDesigners2,
        [
            title("Game Designers"),
            name("Suguru Nakatsui"),
            name("Tetsuji Ohta"),
            name("Hitomi Sato"),
            name("Kenji Matsushima"),
        ],
    ),
    page(
        CreditsPageId::GameDesigners3,
        [
            title("Game Designers"),
            name("Junichi Masuda"),
            name("Koji Nishino"),
            name("Shigeru Ohmori"),
            name("Tadashi Takahashi"),
        ],
    ),
    page(
        CreditsPageId::ScenarioPlot,
        [
            title("Scenario Plot"),
            name("Akihito Tomisawa"),
            name("Junichi Masuda"),
            name("Koji Nishino"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Scenario,
        [
            title("Scenario"),
            name("Akihito Tomisawa"),
            name("Hitomi Sato"),
            name("Toshinobu Matsumiya"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::ScriptDesigners,
        [
            title("Script Designers"),
            name("Tomomichi Ohta"),
            name("Satoshi Nohara"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::MapDesigners,
        [
            title("Map Designers"),
            name("Suguru Nakatsui"),
            name("Teruyuki Shimoyamada"),
            name("Shigeru Ohmori"),
            name("Tetsuji Ohta"),
        ],
    ),
    page(
        CreditsPageId::BattleFrontierData,
        [
            BLANK,
            title("Battle Frontier Data"),
            name("Tetsuji Ohta"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::ParametricDesigners,
        [
            title("Parametric Designers"),
            name("Teruyuki Shimoyamada"),
            name("Shigeki Morimoto"),
            name("Tetsuji Ohta"),
            name("Koji Nishino"),
        ],
    ),
    page(
        CreditsPageId::PokedexText,
        [
            BLANK,
            title("POKéDEX Text"),
            name("Kenji Matsushima"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::EnvironmentAndToolPrograms1,
        [
            title("Environment & Tool Programmers"),
            name("Hisashi Sogabe"),
            name("Sosuke Tamada"),
            name("Hiroyuki Nakamura"),
            name("Akito Mori"),
        ],
    ),
    page(
        CreditsPageId::PkmnDesigners1,
        [
            title("POKéMON Designers"),
            name("Ken Sugimori"),
            name("Motofumi Fujiwara"),
            name("Shigeki Morimoto"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::PkmnDesigners2,
        [
            title("POKéMON Designers"),
            name("Hironobu Yoshida"),
            name("Satoshi Ohta"),
            name("Asuka Iwashita"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::PkmnDesigners3,
        [
            title("POKéMON Designers"),
            name("Takao Unno"),
            name("Kanako Eo"),
            name("Aimi Tomita"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::PkmnDesigners4,
        [
            title("POKéMON Designers"),
            name("Atsuko Nishida"),
            name("Muneo Saito"),
            name("Rena Yoshikawa"),
            name("Jun Okutani"),
        ],
    ),
    page(
        CreditsPageId::SupportProgrammers,
        [
            BLANK,
            title("Support Programmers"),
            name("Satoshi Mitsuhara"),
            name("Daisuke Hoshino"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::NclProductTesting,
        [
            BLANK,
            title("NCL Product Testing"),
            name("NCL Super Mario Club"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::PackageAndManual,
        [
            BLANK,
            title("Package & Manual Illustration"),
            name("Ken Sugimori"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::SpecialThanks1,
        [
            BLANK,
            title("Special Thanks"),
            name("Kenji Tominaga"),
            name("Hiroki Enomoto"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::SpecialThanks2,
        [
            title("Special Thanks"),
            name("Kazuya Suyama"),
            name("Kenjiro Ito"),
            name("Michiko Takizawa"),
            name("Makiko Takada"),
        ],
    ),
    page(
        CreditsPageId::SpecialThanks3,
        [
            title("Special Thanks"),
            name("Mikiko Ohhashi"),
            name("Takanao Kondo"),
            name("Rui Kawaguchi"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::SpecialThanks4,
        [
            title("Special Thanks"),
            name("Takahiro Ohnishi"),
            name("Norihide Okamura"),
            name("Shunsuke Kohori"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::InformationSupervisors,
        [
            title("Information Supervisors"),
            name("Kazuyuki Terada"),
            name("Yuri Sakurai"),
            name("Yumi Funasaka"),
            name("Naoko Yanase"),
        ],
    ),
    page(
        CreditsPageId::Artwork1,
        [
            BLANK,
            title("Artwork"),
            name("Sachiko Nakamichi"),
            name("Fujiko Nomura"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Artwork2,
        [
            BLANK,
            title("Artwork"),
            name("Hideyuki Nakajima"),
            name("Hidenori Saeki"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Artwork3,
        [
            title("Artwork"),
            name("Yoko Watanabe"),
            name("Sakae Kimura"),
            name("Chiaki Shinkai"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Coordinators,
        [
            title("Coordinators"),
            name("Kazuki Yoshihara"),
            name("Akira Kinashi"),
            name("Retsuji Nomoto"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::EnglishVersion,
        [
            BLANK,
            title("English Version Coordinators"),
            name("Hiro Nakamura"),
            name("Seth McMahill"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Translator,
        [
            BLANK,
            title("Translator"),
            name("Nob Ogasawara"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::TextEditor,
        [
            BLANK,
            title("Text Editor"),
            name("Teresa Lillygren"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::NclCoordinator,
        [
            BLANK,
            title("NCL Coordinator"),
            name("Kimiko Nakamichi"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Programmers5,
        [
            title("Programmers"),
            name("Teruki Murakawa"),
            name("Souichi Yamamoto"),
            name("Yuichiro Ito"),
            name("Akira Kinashi"),
        ],
    ),
    page(
        CreditsPageId::GraphicDesigner,
        [
            BLANK,
            title("Graphic Designer"),
            name("Akira Kinashi"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::EnvironmentAndToolPrograms2,
        [
            title("Environment & Tool Programmers"),
            name("Teruki Murakawa"),
            name("Souichi Yamamoto"),
            name("Kimiko Nakamichi"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::NoaTesting,
        [
            title("NOA Product Testing"),
            name("Thomas Hertzog"),
            name("Erik Johnson"),
            name("Mika Kurosawa"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::BrailleCodeCheck1,
        [
            title("Braille Code Check"),
            name("National Federation of the Blind"),
            name("Patricia A. Maurer"),
            name("Japan Braille Library"),
            name("European Blind Union"),
        ],
    ),
    page(
        CreditsPageId::BrailleCodeCheck2,
        [
            BLANK,
            title("Braille Code Check"),
            name("Australian Braille Authority"),
            name("Royal New Zealand Federation for the Blind"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::SpecialThanks5,
        [
            title("Special Thanks"),
            name("Hiroyuki Uesugi"),
            name("Motoyasu Tojima"),
            name("Nicola Pratt-Barlow"),
            name("Shellie Dow"),
        ],
    ),
    page(
        CreditsPageId::TaskManagers,
        [
            BLANK,
            title("Task Managers"),
            name("Azusa Tajima"),
            name("Shusaku Egami"),
            BLANK,
        ],
    ),
    page(
        CreditsPageId::Producers,
        [
            title("Producers"),
            name("Hiroyuki Jinnai"),
            name("Hitoshi Yamagami"),
            name("Gakuji Nomoto"),
            name("Hiroaki Tsuru"),
        ],
    ),
    page(
        CreditsPageId::ExecutiveDirector,
        [
            BLANK,
            title("Executive Director"),
            name("Satoshi Tajiri"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::ExecutiveProducers1,
        [
            BLANK,
            title("Executive Producers"),
            name("Satoru Iwata"),
            BLANK,
            BLANK,
        ],
    ),
    page(
        CreditsPageId::ExecutiveProducers2,
        [
            BLANK,
            title("Executive Producers"),
            name("Tsunekaz Ishihara"),
            BLANK,
            BLANK,
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Page ids in upstream enum order.
    const PAGE_NAMES: [&str; CREDITS_PAGE_COUNT] = [
        "Title",
        "Director",
        "ArtDirector",
        "WorldDirector",
        "LeadProgrammer",
        "Programmers1",
        "Programmers2",
        "Programmers3",
        "Programmers4",
        "GraphicDesigners1",
        "GraphicDesigners2",
        "GraphicDesigners3",
        "MusicComposition",
        "SoundEffects",
        "GameDesigners1",
        "GameDesigners2",
        "GameDesigners3",
        "ScenarioPlot",
        "Scenario",
        "ScriptDesigners",
        "MapDesigners",
        "BattleFrontierData",
        "ParametricDesigners",
        "PokedexText",
        "EnvironmentAndToolPrograms1",
        "PkmnDesigners1",
        "PkmnDesigners2",
        "PkmnDesigners3",
        "PkmnDesigners4",
        "SupportProgrammers",
        "NclProductTesting",
        "PackageAndManual",
        "SpecialThanks1",
        "SpecialThanks2",
        "SpecialThanks3",
        "SpecialThanks4",
        "InformationSupervisors",
        "Artwork1",
        "Artwork2",
        "Artwork3",
        "Coordinators",
        "EnglishVersion",
        "Translator",
        "TextEditor",
        "NclCoordinator",
        "Programmers5",
        "GraphicDesigner",
        "EnvironmentAndToolPrograms2",
        "NoaTesting",
        "BrailleCodeCheck1",
        "BrailleCodeCheck2",
        "SpecialThanks5",
        "TaskManagers",
        "Producers",
        "ExecutiveDirector",
        "ExecutiveProducers1",
        "ExecutiveProducers2",
    ];

    /// One `T|text` (title) or `N|text` (name) row per slot, five per page,
    /// read from the compiled upstream table.
    const EXPECTED_SLOTS: &str = "\
N|\n\
T|POKéMON EMERALD VERSION\n\
T|Credits\n\
N|\n\
N|\n\
N|\n\
T|Director\n\
N|Shigeki Morimoto\n\
N|\n\
N|\n\
N|\n\
T|Art Director\n\
N|Ken Sugimori\n\
N|\n\
N|\n\
N|\n\
T|World Director\n\
N|Junichi Masuda\n\
N|\n\
N|\n\
T|Lead Programmer\n\
N|Hisashi Sogabe\n\
T|Lead Graphic Artist\n\
N|Motofumi Fujiwara\n\
N|\n\
T|Programmers\n\
N|Hisashi Sogabe\n\
N|Tomomichi Ohta\n\
N|Nozomu Saito\n\
N|\n\
T|Programmers\n\
N|Akito Mori\n\
N|Hiroyuki Nakamura\n\
N|Masao Taya\n\
N|\n\
T|Programmers\n\
N|Satoshi Nohara\n\
N|Miyuki Iwasawa\n\
N|Yoshinori Matsuda\n\
N|Keita Kagaya\n\
T|Programmers\n\
N|Tetsuya Watanabe\n\
N|Sosuke Tamada\n\
N|Takenori Ohta\n\
N|\n\
N|\n\
T|Graphic Designers\n\
N|Motofumi Fujiwara\n\
N|Satoshi Ohta\n\
N|\n\
T|Graphic Designers\n\
N|Kenkichi Toyama\n\
N|Asuka Iwashita\n\
N|Takao Unno\n\
N|\n\
T|Graphic Designers\n\
N|Ken Sugimori\n\
N|Hironobu Yoshida\n\
N|Aimi Tomita\n\
N|Kanako Eo\n\
T|Music Composition\n\
N|Go Ichinose\n\
N|Junichi Masuda\n\
N|Morikazu Aoki\n\
N|Hitomi Sato\n\
N|\n\
T|Sound Effects & POKéMON Voices\n\
N|Go Ichinose\n\
N|Morikazu Aoki\n\
N|\n\
T|Game Designers\n\
N|Shigeki Morimoto\n\
N|Teruyuki Shimoyamada\n\
N|Takeshi Kawachimaru\n\
N|Akihito Tomisawa\n\
T|Game Designers\n\
N|Suguru Nakatsui\n\
N|Tetsuji Ohta\n\
N|Hitomi Sato\n\
N|Kenji Matsushima\n\
T|Game Designers\n\
N|Junichi Masuda\n\
N|Koji Nishino\n\
N|Shigeru Ohmori\n\
N|Tadashi Takahashi\n\
T|Scenario Plot\n\
N|Akihito Tomisawa\n\
N|Junichi Masuda\n\
N|Koji Nishino\n\
N|\n\
T|Scenario\n\
N|Akihito Tomisawa\n\
N|Hitomi Sato\n\
N|Toshinobu Matsumiya\n\
N|\n\
T|Script Designers\n\
N|Tomomichi Ohta\n\
N|Satoshi Nohara\n\
N|\n\
N|\n\
T|Map Designers\n\
N|Suguru Nakatsui\n\
N|Teruyuki Shimoyamada\n\
N|Shigeru Ohmori\n\
N|Tetsuji Ohta\n\
N|\n\
T|Battle Frontier Data\n\
N|Tetsuji Ohta\n\
N|\n\
N|\n\
T|Parametric Designers\n\
N|Teruyuki Shimoyamada\n\
N|Shigeki Morimoto\n\
N|Tetsuji Ohta\n\
N|Koji Nishino\n\
N|\n\
T|POKéDEX Text\n\
N|Kenji Matsushima\n\
N|\n\
N|\n\
T|Environment & Tool Programmers\n\
N|Hisashi Sogabe\n\
N|Sosuke Tamada\n\
N|Hiroyuki Nakamura\n\
N|Akito Mori\n\
T|POKéMON Designers\n\
N|Ken Sugimori\n\
N|Motofumi Fujiwara\n\
N|Shigeki Morimoto\n\
N|\n\
T|POKéMON Designers\n\
N|Hironobu Yoshida\n\
N|Satoshi Ohta\n\
N|Asuka Iwashita\n\
N|\n\
T|POKéMON Designers\n\
N|Takao Unno\n\
N|Kanako Eo\n\
N|Aimi Tomita\n\
N|\n\
T|POKéMON Designers\n\
N|Atsuko Nishida\n\
N|Muneo Saito\n\
N|Rena Yoshikawa\n\
N|Jun Okutani\n\
N|\n\
T|Support Programmers\n\
N|Satoshi Mitsuhara\n\
N|Daisuke Hoshino\n\
N|\n\
N|\n\
T|NCL Product Testing\n\
N|NCL Super Mario Club\n\
N|\n\
N|\n\
N|\n\
T|Package & Manual Illustration\n\
N|Ken Sugimori\n\
N|\n\
N|\n\
N|\n\
T|Special Thanks\n\
N|Kenji Tominaga\n\
N|Hiroki Enomoto\n\
N|\n\
T|Special Thanks\n\
N|Kazuya Suyama\n\
N|Kenjiro Ito\n\
N|Michiko Takizawa\n\
N|Makiko Takada\n\
T|Special Thanks\n\
N|Mikiko Ohhashi\n\
N|Takanao Kondo\n\
N|Rui Kawaguchi\n\
N|\n\
T|Special Thanks\n\
N|Takahiro Ohnishi\n\
N|Norihide Okamura\n\
N|Shunsuke Kohori\n\
N|\n\
T|Information Supervisors\n\
N|Kazuyuki Terada\n\
N|Yuri Sakurai\n\
N|Yumi Funasaka\n\
N|Naoko Yanase\n\
N|\n\
T|Artwork\n\
N|Sachiko Nakamichi\n\
N|Fujiko Nomura\n\
N|\n\
N|\n\
T|Artwork\n\
N|Hideyuki Nakajima\n\
N|Hidenori Saeki\n\
N|\n\
T|Artwork\n\
N|Yoko Watanabe\n\
N|Sakae Kimura\n\
N|Chiaki Shinkai\n\
N|\n\
T|Coordinators\n\
N|Kazuki Yoshihara\n\
N|Akira Kinashi\n\
N|Retsuji Nomoto\n\
N|\n\
N|\n\
T|English Version Coordinators\n\
N|Hiro Nakamura\n\
N|Seth McMahill\n\
N|\n\
N|\n\
T|Translator\n\
N|Nob Ogasawara\n\
N|\n\
N|\n\
N|\n\
T|Text Editor\n\
N|Teresa Lillygren\n\
N|\n\
N|\n\
N|\n\
T|NCL Coordinator\n\
N|Kimiko Nakamichi\n\
N|\n\
N|\n\
T|Programmers\n\
N|Teruki Murakawa\n\
N|Souichi Yamamoto\n\
N|Yuichiro Ito\n\
N|Akira Kinashi\n\
N|\n\
T|Graphic Designer\n\
N|Akira Kinashi\n\
N|\n\
N|\n\
T|Environment & Tool Programmers\n\
N|Teruki Murakawa\n\
N|Souichi Yamamoto\n\
N|Kimiko Nakamichi\n\
N|\n\
T|NOA Product Testing\n\
N|Thomas Hertzog\n\
N|Erik Johnson\n\
N|Mika Kurosawa\n\
N|\n\
T|Braille Code Check\n\
N|National Federation of the Blind\n\
N|Patricia A. Maurer\n\
N|Japan Braille Library\n\
N|European Blind Union\n\
N|\n\
T|Braille Code Check\n\
N|Australian Braille Authority\n\
N|Royal New Zealand Federation for the Blind\n\
N|\n\
T|Special Thanks\n\
N|Hiroyuki Uesugi\n\
N|Motoyasu Tojima\n\
N|Nicola Pratt-Barlow\n\
N|Shellie Dow\n\
N|\n\
T|Task Managers\n\
N|Azusa Tajima\n\
N|Shusaku Egami\n\
N|\n\
T|Producers\n\
N|Hiroyuki Jinnai\n\
N|Hitoshi Yamagami\n\
N|Gakuji Nomoto\n\
N|Hiroaki Tsuru\n\
N|\n\
T|Executive Director\n\
N|Satoshi Tajiri\n\
N|\n\
N|\n\
N|\n\
T|Executive Producers\n\
N|Satoru Iwata\n\
N|\n\
N|\n\
N|\n\
T|Executive Producers\n\
N|Tsunekaz Ishihara\n\
N|\n\
N|\n\
";

    fn expected(page: usize, slot: usize) -> (bool, &'static str) {
        let row = EXPECTED_SLOTS
            .lines()
            .nth(page * CREDITS_ENTRIES_PER_PAGE + slot)
            .expect("expected slot present");
        let (flag, text) = row.split_once('|').expect("flag separator");
        (flag == "T", text)
    }

    #[test]
    fn page_count_and_slots_per_page() {
        assert_eq!(CREDITS_PAGES.len(), 57);
        assert_eq!(EXPECTED_SLOTS.lines().count(), 57 * 5);
        for page in &CREDITS_PAGES {
            assert_eq!(page.entries.len(), 5);
        }
    }

    #[test]
    fn page_order_matches_the_upstream_enum() {
        for (index, page) in CREDITS_PAGES.iter().enumerate() {
            assert_eq!(format!("{:?}", page.id), PAGE_NAMES[index]);
            assert_eq!(page.id.index(), index);
            assert_eq!(CreditsPageId::ALL[index], page.id);
        }
        assert_eq!(CreditsPageId::ALL.len(), CREDITS_PAGE_COUNT);
    }

    #[test]
    fn every_slot_matches_the_independent_expected_data() {
        for (index, page) in CREDITS_PAGES.iter().enumerate() {
            for (slot, entry) in page.entries.iter().enumerate() {
                let (is_title, text) = expected(index, slot);
                assert_eq!(entry.text, text, "page {index} slot {slot} text");
                assert_eq!(entry.is_title, is_title, "page {index} slot {slot} flag");
            }
        }
    }

    #[test]
    fn blank_slots_are_preserved_in_place() {
        let title = credits_page(CreditsPageId::Title.index()).unwrap();
        let shape: Vec<bool> = title.entries.iter().map(|e| e.text.is_empty()).collect();
        assert_eq!(shape, [true, false, false, true, true]);

        let programmers3 = credits_page(CreditsPageId::Programmers3.index()).unwrap();
        assert!(programmers3.entries.iter().all(|e| !e.text.is_empty()));

        let blanks: usize = CREDITS_PAGES
            .iter()
            .flat_map(|p| p.entries.iter())
            .filter(|e| e.text.is_empty())
            .count();
        assert_eq!(
            blanks,
            EXPECTED_SLOTS.lines().filter(|l| l.ends_with('|')).count()
        );
        for entry in CREDITS_PAGES.iter().flat_map(|p| p.entries.iter()) {
            if entry.text.is_empty() {
                assert!(!entry.is_title);
            }
        }
    }

    #[test]
    fn title_flags_style_headings_not_names() {
        let director = credits_page(CreditsPageId::Director.index()).unwrap();
        let flags: Vec<bool> = director.entries.iter().map(|e| e.is_title).collect();
        assert_eq!(flags, [false, true, false, false, false]);
        assert_eq!(director.entries[1].text, "Director");
        assert_eq!(director.entries[2].text, "Shigeki Morimoto");

        let titles = CREDITS_PAGES
            .iter()
            .flat_map(|p| p.entries.iter())
            .filter(|e| e.is_title)
            .count();
        assert_eq!(titles, 59);
    }

    #[test]
    fn english_names_and_punctuation_are_exact() {
        let text = |id: CreditsPageId, slot: usize| CREDITS_PAGES[id.index()].entries[slot].text;
        assert_eq!(text(CreditsPageId::Title, 1), "POKéMON EMERALD VERSION");
        assert_eq!(
            text(CreditsPageId::SoundEffects, 1),
            "Sound Effects & POKéMON Voices"
        );
        assert_eq!(text(CreditsPageId::PokedexText, 1), "POKéDEX Text");
        assert_eq!(
            text(CreditsPageId::PackageAndManual, 1),
            "Package & Manual Illustration"
        );
        assert!(CREDITS_PAGES
            .iter()
            .flat_map(|p| p.entries.iter())
            .any(|e| e.text == "Patricia A. Maurer" && !e.is_title));
        assert!(CREDITS_PAGES
            .iter()
            .flat_map(|p| p.entries.iter())
            .any(|e| e.text == "Nicola Pratt-Barlow" && !e.is_title));
    }

    #[test]
    fn unreferenced_upstream_entries_are_kept_in_declaration_order() {
        const EXPECTED: &str = "\
T|Battle Director\n\
T|Main Programmer\n\
T|Battle System Programmers\n\
T|Field System Programmer\n\
T|Main Graphic Designer\n\
T|Map Data Designers\n\
N|Hiromi Sagawa\n\
N|Yoshio Tajiri\n\
N|Teiko Sasaki\n\
N|Sachiko Hamano\n\
N|Chie Matsumiya\n\
N|Akiko Shinozaki\n\
N|Astuko Fujii\n\
N|Atsushi Tada\n\
N|Ai Mashima\n\
N|Takehiro Izushi\n\
N|Kyoko Watanabe\n\
N|Takao Nakano\n\
N|Tomotaka Komura\n\
";
        assert_eq!(CREDITS_UNUSED_ENTRIES.len(), 19);
        for (entry, row) in CREDITS_UNUSED_ENTRIES.iter().zip(EXPECTED.lines()) {
            let (flag, text) = row.split_once('|').expect("flag separator");
            assert_eq!(entry.text, text);
            assert_eq!(entry.is_title, flag == "T");
        }
        assert_eq!(EXPECTED.lines().count(), CREDITS_UNUSED_ENTRIES.len());
    }

    #[test]
    fn page_lookup_rejects_an_invalid_index() {
        assert_eq!(credits_page(0).unwrap().id, CreditsPageId::Title);
        assert_eq!(
            credits_page(CREDITS_PAGE_COUNT - 1).unwrap().id,
            CreditsPageId::ExecutiveProducers2
        );
        assert_eq!(
            credits_page(CREDITS_PAGE_COUNT),
            Err(CreditsError::UnknownPage(57))
        );
        assert_eq!(
            credits_page(usize::MAX),
            Err(CreditsError::UnknownPage(usize::MAX))
        );
        assert_eq!(
            CreditsError::UnknownPage(57).to_string(),
            "unknown credits page `57`"
        );
    }
}
