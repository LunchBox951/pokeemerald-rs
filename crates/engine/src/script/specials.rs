//! The `gSpecials` name table (S-5, slice 2, issue #93).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the *shape* of
//! upstream's `gSpecials` (`pokeemerald/data/specials.inc` +
//! `pokeemerald/data/event_scripts.s:92-94`'s `ALLOCATE_SPECIAL_TABLE`
//! inclusion that materializes it): a flat, order-significant table of C
//! function names `special`/`specialvar` (`pokeemerald/src/scrcmd.c`'s
//! `ScrCmd_special`/`ScrCmd_specialvar`) index into by a `SPECIAL_<name>`
//! constant the assembler resolves to that name's position in the table
//! (`asm/macros/event.inc:279-297`'s `special`/`specialvar` macros). Only the
//! *names* (plus each one's `waitstate` flag) are transcribed here, in
//! upstream's exact table order, as data — every [`SPECIAL_TABLE`] slot
//! is `None` in this slice `(no-verbatim)`: none of the ~30 subsystems those
//! 527 C functions reach into (secret bases, link-cable trading, the Pokédex,
//! contests, the Battle Frontier, …) exist in this port yet. A later slice
//! fills in a slot at a time as its subsystem lands, exactly like
//! [`crate::script::commands::COMMAND_TABLE`]'s `unimplemented::<OP>`
//! placeholders get replaced one opcode at a time.
//!
//! Three names appear at two different [`SPECIALS`] indices each —
//! `ShowGlassWorkshopMenu` (277, 348), `ShowMapNamePopup` (409, 410), and
//! `Script_DoRayquazaScene` (470, 508, the latter upstream-commented `@
//! Listed twice`) — transcribed faithfully rather than deduplicated, since
//! each reflects two distinct `def_special` lines upstream itself repeats
//! (`.set SPECIAL_<name>` simply ends up bound to the *second* occurrence's
//! index for those three names, an upstream quirk this table preserves
//! positionally without trying to resolve).

/// One `data/specials.inc` `def_special` entry: the upstream C function's
/// name, and whether calling it through `special`/`specialvar` implicitly
/// appends a `waitstate` (upstream `SPECIAL_WAITSTATE_<name>`,
/// `asm/macros/event.inc:283-284,294-295`) — i.e. whether the *caller* (a
/// compiled script, out of scope here) blocks until the special completes
/// asynchronously. Recorded for when script *compilation* exists to consume
/// it; [`SPECIAL_TABLE`] dispatch itself does not consult it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialSpec {
    /// The upstream C function name, e.g. `"HealPlayerParty"`.
    pub name: &'static str,
    /// `SPECIAL_WAITSTATE_<name>` upstream: `true` if calling this special
    /// implicitly generates a `waitstate`.
    pub waitstate: bool,
}

/// The number of entries in upstream's `gSpecials` (`data/specials.inc`).
pub const SPECIAL_COUNT: usize = 527;

/// Transcribed 1:1 from `data/specials.inc`, in table order — ground truth,
/// not derived from any logic. See the module docs for the three
/// upstream-duplicated names.
#[rustfmt::skip]
pub const SPECIALS: [SpecialSpec; SPECIAL_COUNT] = [
    SpecialSpec { name: "HealPlayerParty", waitstate: false },
    SpecialSpec { name: "SetCableClubWarp", waitstate: false },
    SpecialSpec { name: "DoCableClubWarp", waitstate: true },
    SpecialSpec { name: "ReturnFromLinkRoom", waitstate: true },
    SpecialSpec { name: "CleanupLinkRoomState", waitstate: false },
    SpecialSpec { name: "ExitLinkRoom", waitstate: false },
    SpecialSpec { name: "SetPlayerSecretBase", waitstate: false },
    SpecialSpec { name: "CheckPlayerHasSecretBase", waitstate: false },
    SpecialSpec { name: "EnterSecretBase", waitstate: false },
    SpecialSpec { name: "ClearAndLeaveSecretBase", waitstate: false },
    SpecialSpec { name: "MoveOutOfSecretBase", waitstate: false },
    SpecialSpec { name: "IsCurSecretBaseOwnedByAnotherPlayer", waitstate: false },
    SpecialSpec { name: "GetCurSecretBaseRegistrationValidity", waitstate: false },
    SpecialSpec { name: "ToggleCurSecretBaseRegistry", waitstate: false },
    SpecialSpec { name: "ShowSecretBaseDecorationMenu", waitstate: false },
    SpecialSpec { name: "ShowSecretBaseRegistryMenu", waitstate: false },
    SpecialSpec { name: "PrepSecretBaseBattleFlags", waitstate: false },
    SpecialSpec { name: "GetSecretBaseOwnerAndState", waitstate: false },
    SpecialSpec { name: "InitSecretBaseDecorationSprites", waitstate: false },
    SpecialSpec { name: "SetDecoration", waitstate: false },
    SpecialSpec { name: "GetObjectEventLocalIdByFlag", waitstate: false },
    SpecialSpec { name: "GetSecretBaseTypeInFrontOfPlayer", waitstate: false },
    SpecialSpec { name: "SetSecretBaseOwnerGfxId", waitstate: false },
    SpecialSpec { name: "PutAwayDecorationIteration", waitstate: false },
    SpecialSpec { name: "EnterNewlyCreatedSecretBase", waitstate: true },
    SpecialSpec { name: "SetBattledOwnerFromResult", waitstate: false },
    SpecialSpec { name: "DoSecretBasePCTurnOffEffect", waitstate: false },
    SpecialSpec { name: "RecordMixingPlayerSpotTriggered", waitstate: true },
    SpecialSpec { name: "TryBattleLinkup", waitstate: true },
    SpecialSpec { name: "TryTradeLinkup", waitstate: true },
    SpecialSpec { name: "TryRecordMixLinkup", waitstate: true },
    SpecialSpec { name: "ValidateMixingGameLanguage", waitstate: true },
    SpecialSpec { name: "CloseLink", waitstate: false },
    SpecialSpec { name: "ColosseumPlayerSpotTriggered", waitstate: true },
    SpecialSpec { name: "PlayerEnteredTradeSeat", waitstate: true },
    SpecialSpec { name: "Script_StartWiredTrade", waitstate: false },
    SpecialSpec { name: "CableClubSaveGame", waitstate: false },
    SpecialSpec { name: "TryBerryBlenderLinkup", waitstate: true },
    SpecialSpec { name: "GetLinkPartnerNames", waitstate: false },
    SpecialSpec { name: "SpawnLinkPartnerObjectEvent", waitstate: false },
    SpecialSpec { name: "SavePlayerParty", waitstate: false },
    SpecialSpec { name: "LoadPlayerParty", waitstate: false },
    SpecialSpec { name: "ChooseHalfPartyForBattle", waitstate: true },
    SpecialSpec { name: "Script_ShowLinkTrainerCard", waitstate: true },
    SpecialSpec { name: "ObjectEventInteractionGetBerryTreeData", waitstate: false },
    SpecialSpec { name: "ObjectEventInteractionGetBerryName", waitstate: false },
    SpecialSpec { name: "ObjectEventInteractionGetBerryCountString", waitstate: false },
    SpecialSpec { name: "Bag_ChooseBerry", waitstate: true },
    SpecialSpec { name: "ObjectEventInteractionPlantBerryTree", waitstate: false },
    SpecialSpec { name: "ObjectEventInteractionPickBerryTree", waitstate: false },
    SpecialSpec { name: "ObjectEventInteractionRemoveBerryTree", waitstate: false },
    SpecialSpec { name: "ObjectEventInteractionWaterBerryTree", waitstate: false },
    SpecialSpec { name: "PlayerHasBerries", waitstate: false },
    SpecialSpec { name: "IsEnigmaBerryValid", waitstate: false },
    SpecialSpec { name: "GetTrainerBattleMode", waitstate: false },
    SpecialSpec { name: "ShowTrainerIntroSpeech", waitstate: false },
    SpecialSpec { name: "ShowTrainerCantBattleSpeech", waitstate: false },
    SpecialSpec { name: "GetTrainerFlag", waitstate: false },
    SpecialSpec { name: "DoTrainerApproach", waitstate: true },
    SpecialSpec { name: "PlayTrainerEncounterMusic", waitstate: false },
    SpecialSpec { name: "ShouldTryRematchBattle", waitstate: false },
    SpecialSpec { name: "IsTrainerReadyForRematch", waitstate: false },
    SpecialSpec { name: "BattleSetup_StartRematchBattle", waitstate: true },
    SpecialSpec { name: "ShowPokemonStorageSystemPC", waitstate: true },
    SpecialSpec { name: "HasEnoughMonsForDoubleBattle", waitstate: false },
    SpecialSpec { name: "TurnOffTVScreen", waitstate: false },
    SpecialSpec { name: "DoTVShow", waitstate: false },
    SpecialSpec { name: "DoPokeNews", waitstate: false },
    SpecialSpec { name: "GetRandomActiveShowIdx", waitstate: false },
    SpecialSpec { name: "GetSelectedTVShow", waitstate: false },
    SpecialSpec { name: "InterviewBefore", waitstate: false },
    SpecialSpec { name: "InterviewAfter", waitstate: false },
    SpecialSpec { name: "IsLeadMonNicknamedOrNotEnglish", waitstate: false },
    SpecialSpec { name: "SetContestCategoryStringVarForInterview", waitstate: false },
    SpecialSpec { name: "GetNextActiveShowIfMassOutbreak", waitstate: false },
    SpecialSpec { name: "IsTVShowAlreadyInQueue", waitstate: false },
    SpecialSpec { name: "CheckForPlayersHouseNews", waitstate: false },
    SpecialSpec { name: "GetMomOrDadStringForTVMessage", waitstate: false },
    SpecialSpec { name: "ResetTVShowState", waitstate: false },
    SpecialSpec { name: "GetContestWinnerId", waitstate: false },
    SpecialSpec { name: "GetContestPlayerId", waitstate: false },
    SpecialSpec { name: "GetNpcContestantLocalId", waitstate: false },
    SpecialSpec { name: "BufferContestWinnerTrainerName", waitstate: false },
    SpecialSpec { name: "BufferContestWinnerMonName", waitstate: false },
    SpecialSpec { name: "BufferContestTrainerAndMonNames", waitstate: false },
    SpecialSpec { name: "GetContestMonConditionRanking", waitstate: false },
    SpecialSpec { name: "SetContestTrainerGfxIds", waitstate: false },
    SpecialSpec { name: "TryEnterContestMon", waitstate: false },
    SpecialSpec { name: "GetContestantNamesAtRank", waitstate: false },
    SpecialSpec { name: "SetLinkContestPlayerGfx", waitstate: false },
    SpecialSpec { name: "GetContestMonCondition", waitstate: false },
    SpecialSpec { name: "HasMonWonThisContestBefore", waitstate: false },
    SpecialSpec { name: "GiveMonContestRibbon", waitstate: false },
    SpecialSpec { name: "IsContestDebugActive", waitstate: false },
    SpecialSpec { name: "GiveMonArtistRibbon", waitstate: false },
    SpecialSpec { name: "TryContestGModeLinkup", waitstate: true },
    SpecialSpec { name: "SaveGame", waitstate: true },
    SpecialSpec { name: "DoWateringBerryTreeAnim", waitstate: true },
    SpecialSpec { name: "ShowEasyChatScreen", waitstate: false },
    SpecialSpec { name: "ShowEasyChatProfile", waitstate: false },
    SpecialSpec { name: "Script_GetCurrentMauvilleMan", waitstate: false },
    SpecialSpec { name: "HasBardSongBeenChanged", waitstate: false },
    SpecialSpec { name: "SaveBardSongLyrics", waitstate: false },
    SpecialSpec { name: "HasHipsterTaughtWord", waitstate: false },
    SpecialSpec { name: "SetHipsterTaughtWord", waitstate: false },
    SpecialSpec { name: "HipsterTryTeachWord", waitstate: false },
    SpecialSpec { name: "PlayBardSong", waitstate: false },
    SpecialSpec { name: "SetMauvilleOldManObjEventGfx", waitstate: false },
    SpecialSpec { name: "GenerateGiddyLine", waitstate: false },
    SpecialSpec { name: "GiddyShouldTellAnotherTale", waitstate: false },
    SpecialSpec { name: "StorytellerGetFreeStorySlot", waitstate: false },
    SpecialSpec { name: "Script_StorytellerDisplayStory", waitstate: false },
    SpecialSpec { name: "StorytellerStoryListMenu", waitstate: true },
    SpecialSpec { name: "StorytellerUpdateStat", waitstate: false },
    SpecialSpec { name: "Script_StorytellerInitializeRandomStat", waitstate: false },
    SpecialSpec { name: "HasStorytellerAlreadyRecorded", waitstate: false },
    SpecialSpec { name: "TraderMenuGetDecoration", waitstate: true },
    SpecialSpec { name: "GetTraderTradedFlag", waitstate: false },
    SpecialSpec { name: "DoesPlayerHaveNoDecorations", waitstate: false },
    SpecialSpec { name: "IsDecorationCategoryFull", waitstate: false },
    SpecialSpec { name: "TraderShowDecorationMenu", waitstate: true },
    SpecialSpec { name: "TraderDoDecorationTrade", waitstate: false },
    SpecialSpec { name: "GetSeedotSizeRecordInfo", waitstate: false },
    SpecialSpec { name: "CompareSeedotSize", waitstate: false },
    SpecialSpec { name: "GetLotadSizeRecordInfo", waitstate: false },
    SpecialSpec { name: "CompareLotadSize", waitstate: false },
    SpecialSpec { name: "TryPutNameRaterShowOnTheAir", waitstate: false },
    SpecialSpec { name: "BufferMonNickname", waitstate: false },
    SpecialSpec { name: "IsMonOTIDNotPlayers", waitstate: false },
    SpecialSpec { name: "BufferTrendyPhraseString", waitstate: false },
    SpecialSpec { name: "IsTrendyPhraseBoring", waitstate: false },
    SpecialSpec { name: "BufferDeepLinkPhrase", waitstate: false },
    SpecialSpec { name: "GetDewfordHallPaintingNameIndex", waitstate: false },
    SpecialSpec { name: "SwapRegisteredBike", waitstate: false },
    SpecialSpec { name: "CalculatePlayerPartyCount", waitstate: false },
    SpecialSpec { name: "CountPartyNonEggMons", waitstate: false },
    SpecialSpec { name: "CountPartyAliveNonEggMons_IgnoreVar0x8004Slot", waitstate: false },
    SpecialSpec { name: "ShouldReadyContestArtist", waitstate: false },
    SpecialSpec { name: "SaveMuseumContestPainting", waitstate: false },
    SpecialSpec { name: "DoesContestCategoryHaveMuseumPainting", waitstate: false },
    SpecialSpec { name: "CountPlayerMuseumPaintings", waitstate: false },
    SpecialSpec { name: "ShowContestPainting", waitstate: false },
    SpecialSpec { name: "MauvilleGymSetDefaultBarriers", waitstate: false },
    SpecialSpec { name: "MauvilleGymPressSwitch", waitstate: false },
    SpecialSpec { name: "ShowFieldMessageStringVar4", waitstate: false },
    SpecialSpec { name: "DrawWholeMapView", waitstate: false },
    SpecialSpec { name: "StorePlayerCoordsInVars", waitstate: false },
    SpecialSpec { name: "MauvilleGymDeactivatePuzzle", waitstate: false },
    SpecialSpec { name: "PetalburgGymSlideOpenRoomDoors", waitstate: true },
    SpecialSpec { name: "PetalburgGymUnlockRoomDoors", waitstate: false },
    SpecialSpec { name: "GetPlayerTrainerIdOnesDigit", waitstate: false },
    SpecialSpec { name: "GetPlayerBigGuyGirlString", waitstate: false },
    SpecialSpec { name: "GetRivalSonDaughterString", waitstate: false },
    SpecialSpec { name: "SetHiddenItemFlag", waitstate: false },
    SpecialSpec { name: "CableCarWarp", waitstate: false },
    SpecialSpec { name: "CableCar", waitstate: true },
    SpecialSpec { name: "Overworld_PlaySpecialMapMusic", waitstate: false },
    SpecialSpec { name: "StartWallClock", waitstate: true },
    SpecialSpec { name: "Special_ViewWallClock", waitstate: true },
    SpecialSpec { name: "ChooseStarter", waitstate: true },
    SpecialSpec { name: "StartWallyTutorialBattle", waitstate: true },
    SpecialSpec { name: "ChangePokemonNickname", waitstate: true },
    SpecialSpec { name: "ChoosePartyMon", waitstate: true },
    SpecialSpec { name: "GetFirstFreePokeblockSlot", waitstate: false },
    SpecialSpec { name: "DoBerryBlending", waitstate: true },
    SpecialSpec { name: "PlayRoulette", waitstate: true },
    SpecialSpec { name: "IsFanClubMemberFanOfPlayer", waitstate: false },
    SpecialSpec { name: "GetNumFansOfPlayerInTrainerFanClub", waitstate: false },
    SpecialSpec { name: "BufferFanClubTrainerName", waitstate: false },
    SpecialSpec { name: "TryLoseFansFromPlayTimeAfterLinkBattle", waitstate: false },
    SpecialSpec { name: "TryLoseFansFromPlayTime", waitstate: false },
    SpecialSpec { name: "SetPlayerGotFirstFans", waitstate: false },
    SpecialSpec { name: "UpdateTrainerFanClubGameClear", waitstate: false },
    SpecialSpec { name: "Script_TryGainNewFanFromCounter", waitstate: false },
    SpecialSpec { name: "RockSmashWildEncounter", waitstate: false },
    SpecialSpec { name: "GabbyAndTyGetBattleNum", waitstate: false },
    SpecialSpec { name: "GabbyAndTyAfterInterview", waitstate: false },
    SpecialSpec { name: "GabbyAndTyBeforeInterview", waitstate: false },
    SpecialSpec { name: "DoTVShowInSearchOfTrainers", waitstate: false },
    SpecialSpec { name: "IsGabbyAndTyShowOnTheAir", waitstate: false },
    SpecialSpec { name: "GabbyAndTyGetLastQuote", waitstate: false },
    SpecialSpec { name: "GabbyAndTyGetLastBattleTrivia", waitstate: false },
    SpecialSpec { name: "GetGabbyAndTyLocalIds", waitstate: false },
    SpecialSpec { name: "GetBattleOutcome", waitstate: false },
    SpecialSpec { name: "GetDaycareMonNicknames", waitstate: false },
    SpecialSpec { name: "GetDaycareState", waitstate: false },
    SpecialSpec { name: "RejectEggFromDayCare", waitstate: false },
    SpecialSpec { name: "GiveEggFromDaycare", waitstate: false },
    SpecialSpec { name: "SetDaycareCompatibilityString", waitstate: false },
    SpecialSpec { name: "GetSelectedMonNicknameAndSpecies", waitstate: false },
    SpecialSpec { name: "StoreSelectedPokemonInDaycare", waitstate: false },
    SpecialSpec { name: "ChooseSendDaycareMon", waitstate: true },
    SpecialSpec { name: "ShowDaycareLevelMenu", waitstate: true },
    SpecialSpec { name: "GetNumLevelsGainedFromDaycare", waitstate: false },
    SpecialSpec { name: "GetDaycareCostAndPrepareString", waitstate: false },
    SpecialSpec { name: "TakePokemonFromDaycare", waitstate: false },
    SpecialSpec { name: "ScriptHatchMon", waitstate: false },
    SpecialSpec { name: "EggHatch", waitstate: true },
    SpecialSpec { name: "CheckDaycareMonReceivedMail", waitstate: false },
    SpecialSpec { name: "ShowLinkBattleRecords", waitstate: false },
    SpecialSpec { name: "IsEnoughForCostInVar0x8005", waitstate: false },
    SpecialSpec { name: "SubtractMoneyFromVar0x8005", waitstate: false },
    SpecialSpec { name: "TryFieldPoisonWhiteOut", waitstate: true },
    SpecialSpec { name: "SetCB2WhiteOut", waitstate: true },
    SpecialSpec { name: "RotatingGate_InitPuzzle", waitstate: false },
    SpecialSpec { name: "RotatingGate_InitPuzzleAndGraphics", waitstate: false },
    SpecialSpec { name: "SetSSTidalFlag", waitstate: false },
    SpecialSpec { name: "ResetSSTidalFlag", waitstate: false },
    SpecialSpec { name: "EnterSafariMode", waitstate: false },
    SpecialSpec { name: "ExitSafariMode", waitstate: false },
    SpecialSpec { name: "GetPokeblockFeederInFront", waitstate: false },
    SpecialSpec { name: "OpenPokeblockCaseOnFeeder", waitstate: true },
    SpecialSpec { name: "IsMirageIslandPresent", waitstate: false },
    SpecialSpec { name: "UpdateShoalTideFlag", waitstate: false },
    SpecialSpec { name: "InitBirchState", waitstate: false },
    SpecialSpec { name: "ScriptGetPokedexInfo", waitstate: false },
    SpecialSpec { name: "ShowPokedexRatingMessage", waitstate: false },
    SpecialSpec { name: "DoPCTurnOnEffect", waitstate: false },
    SpecialSpec { name: "DoPCTurnOffEffect", waitstate: false },
    SpecialSpec { name: "SetDeptStoreFloor", waitstate: false },
    SpecialSpec { name: "DoLotteryCornerComputerEffect", waitstate: false },
    SpecialSpec { name: "EndLotteryCornerComputerEffect", waitstate: false },
    SpecialSpec { name: "ChooseMonForMoveRelearner", waitstate: true },
    SpecialSpec { name: "MoveDeleterChooseMoveToForget", waitstate: false },
    SpecialSpec { name: "MoveDeleterForgetMove", waitstate: false },
    SpecialSpec { name: "BufferMoveDeleterNicknameAndMove", waitstate: false },
    SpecialSpec { name: "GetNumMovesSelectedMonHas", waitstate: false },
    SpecialSpec { name: "TeachMoveRelearnerMove", waitstate: true },
    SpecialSpec { name: "GetRecordedCyclingRoadResults", waitstate: false },
    SpecialSpec { name: "Special_BeginCyclingRoadChallenge", waitstate: false },
    SpecialSpec { name: "GetPlayerAvatarBike", waitstate: false },
    SpecialSpec { name: "FinishCyclingRoadChallenge", waitstate: false },
    SpecialSpec { name: "UpdateCyclingRoadState", waitstate: false },
    SpecialSpec { name: "GetLeadMonFriendshipScore", waitstate: false },
    SpecialSpec { name: "CallFrontierUtilFunc", waitstate: false },
    SpecialSpec { name: "CallBattleTowerFunc", waitstate: false },
    SpecialSpec { name: "CallBattleDomeFunction", waitstate: false },
    SpecialSpec { name: "CallBattlePalaceFunction", waitstate: false },
    SpecialSpec { name: "CopyEReaderTrainerGreeting", waitstate: false },
    SpecialSpec { name: "DoSpecialTrainerBattle", waitstate: true },
    SpecialSpec { name: "CallBattleArenaFunction", waitstate: false },
    SpecialSpec { name: "CallBattleFactoryFunction", waitstate: false },
    SpecialSpec { name: "CallBattlePikeFunction", waitstate: false },
    SpecialSpec { name: "CallBattlePyramidFunction", waitstate: false },
    SpecialSpec { name: "StopMapMusic", waitstate: false },
    SpecialSpec { name: "CallVerdanturfTentFunction", waitstate: false },
    SpecialSpec { name: "CallFallarborTentFunction", waitstate: false },
    SpecialSpec { name: "CallSlateportTentFunction", waitstate: false },
    SpecialSpec { name: "ChoosePartyForBattleFrontier", waitstate: true },
    SpecialSpec { name: "ValidateEReaderTrainer", waitstate: false },
    SpecialSpec { name: "GetBattleTowerSinglesStreak", waitstate: false },
    SpecialSpec { name: "ReducePlayerPartyToSelectedMons", waitstate: false },
    SpecialSpec { name: "BedroomPC", waitstate: true },
    SpecialSpec { name: "PlayerPC", waitstate: true },
    SpecialSpec { name: "FieldShowRegionMap", waitstate: true },
    SpecialSpec { name: "GetInGameTradeSpeciesInfo", waitstate: false },
    SpecialSpec { name: "CreateInGameTradePokemon", waitstate: false },
    SpecialSpec { name: "DoInGameTradeScene", waitstate: true },
    SpecialSpec { name: "GetTradeSpecies", waitstate: false },
    SpecialSpec { name: "GetWeekCount", waitstate: false },
    SpecialSpec { name: "RetrieveLotteryNumber", waitstate: false },
    SpecialSpec { name: "PickLotteryCornerTicket", waitstate: false },
    SpecialSpec { name: "ShowBerryBlenderRecordWindow", waitstate: false },
    SpecialSpec { name: "ResetTrickHouseNuggetFlag", waitstate: false },
    SpecialSpec { name: "SetTrickHouseNuggetFlag", waitstate: false },
    SpecialSpec { name: "ScriptMenu_CreatePCMultichoice", waitstate: true },
    SpecialSpec { name: "AccessHallOfFamePC", waitstate: true },
    SpecialSpec { name: "Special_ShowDiploma", waitstate: true },
    SpecialSpec { name: "CheckLeadMonCool", waitstate: false },
    SpecialSpec { name: "CheckLeadMonBeauty", waitstate: false },
    SpecialSpec { name: "CheckLeadMonCute", waitstate: false },
    SpecialSpec { name: "CheckLeadMonSmart", waitstate: false },
    SpecialSpec { name: "CheckLeadMonTough", waitstate: false },
    SpecialSpec { name: "LookThroughPorthole", waitstate: true },
    SpecialSpec { name: "DoSoftReset", waitstate: false },
    SpecialSpec { name: "GameClear", waitstate: true },
    SpecialSpec { name: "MoveElevator", waitstate: true },
    SpecialSpec { name: "ShowGlassWorkshopMenu", waitstate: false },
    SpecialSpec { name: "SpawnCameraObject", waitstate: false },
    SpecialSpec { name: "RemoveCameraObject", waitstate: false },
    SpecialSpec { name: "GetPokeblockNameByMonNature", waitstate: false },
    SpecialSpec { name: "GetSecretBaseNearbyMapName", waitstate: false },
    SpecialSpec { name: "CheckRelicanthWailord", waitstate: false },
    SpecialSpec { name: "ShouldDoBrailleRegirockEffectOld", waitstate: false },
    SpecialSpec { name: "DoOrbEffect", waitstate: false },
    SpecialSpec { name: "FadeOutOrbEffect", waitstate: true },
    SpecialSpec { name: "WaitWeather", waitstate: true },
    SpecialSpec { name: "BufferEReaderTrainerName", waitstate: false },
    SpecialSpec { name: "GetSlotMachineId", waitstate: false },
    SpecialSpec { name: "GetPlayerFacingDirection", waitstate: false },
    SpecialSpec { name: "FoundAbandonedShipRoom1Key", waitstate: false },
    SpecialSpec { name: "FoundAbandonedShipRoom2Key", waitstate: false },
    SpecialSpec { name: "FoundAbandonedShipRoom4Key", waitstate: false },
    SpecialSpec { name: "FoundAbandonedShipRoom6Key", waitstate: false },
    SpecialSpec { name: "LeadMonHasEffortRibbon", waitstate: false },
    SpecialSpec { name: "GiveLeadMonEffortRibbon", waitstate: false },
    SpecialSpec { name: "Special_AreLeadMonEVsMaxedOut", waitstate: false },
    SpecialSpec { name: "Script_FacePlayer", waitstate: false },
    SpecialSpec { name: "Script_ClearHeldMovement", waitstate: false },
    SpecialSpec { name: "InitRoamer", waitstate: false },
    SpecialSpec { name: "TryUpdateRusturfTunnelState", waitstate: false },
    SpecialSpec { name: "IsGrassTypeInParty", waitstate: false },
    SpecialSpec { name: "DoContestHallWarp", waitstate: true },
    SpecialSpec { name: "LoadWallyZigzagoon", waitstate: false },
    SpecialSpec { name: "IsStarterInParty", waitstate: false },
    SpecialSpec { name: "CopyCurSecretBaseOwnerName_StrVar1", waitstate: false },
    SpecialSpec { name: "ScriptCheckFreePokemonStorageSpace", waitstate: false },
    SpecialSpec { name: "DoSealedChamberShakingEffect_Long", waitstate: true },
    SpecialSpec { name: "ShowDeptStoreElevatorFloorSelect", waitstate: false },
    SpecialSpec { name: "InteractWithShieldOrTVDecoration", waitstate: false },
    SpecialSpec { name: "IsPokerusInParty", waitstate: false },
    SpecialSpec { name: "SetSootopolisGymCrackedIceMetatiles", waitstate: false },
    SpecialSpec { name: "ShakeCamera", waitstate: false },
    SpecialSpec { name: "StartGroudonKyogreBattle", waitstate: false },
    SpecialSpec { name: "BattleSetup_StartLegendaryBattle", waitstate: true },
    SpecialSpec { name: "StartRegiBattle", waitstate: true },
    SpecialSpec { name: "SetTrainerFacingDirection", waitstate: false },
    SpecialSpec { name: "DoSealedChamberShakingEffect_Short", waitstate: true },
    SpecialSpec { name: "FoundBlackGlasses", waitstate: false },
    SpecialSpec { name: "StartDroughtWeatherBlend", waitstate: false },
    SpecialSpec { name: "DoDiveWarp", waitstate: false },
    SpecialSpec { name: "DoFallWarp", waitstate: true },
    SpecialSpec { name: "ShowContestEntryMonPic", waitstate: false },
    SpecialSpec { name: "HideContestEntryMonPic", waitstate: false },
    SpecialSpec { name: "SetEReaderTrainerGfxId", waitstate: false },
    SpecialSpec { name: "BattleSetup_StartLatiBattle", waitstate: true },
    SpecialSpec { name: "SetRoute119Weather", waitstate: false },
    SpecialSpec { name: "SetRoute123Weather", waitstate: false },
    SpecialSpec { name: "GetContestMultiplayerId", waitstate: false },
    SpecialSpec { name: "ScriptGetPartyMonSpecies", waitstate: false },
    SpecialSpec { name: "IsSelectedMonEgg", waitstate: false },
    SpecialSpec { name: "TryInitBattleTowerAwardManObjectEvent", waitstate: false },
    SpecialSpec { name: "MoveOutOfSecretBaseFromOutside", waitstate: false },
    SpecialSpec { name: "LoadPlayerBag", waitstate: false },
    SpecialSpec { name: "Script_FadeOutMapMusic", waitstate: true },
    SpecialSpec { name: "SetPacifidlogTMReceivedDay", waitstate: false },
    SpecialSpec { name: "GetDaysUntilPacifidlogTMAvailable", waitstate: false },
    SpecialSpec { name: "HasAllHoennMons", waitstate: false },
    SpecialSpec { name: "MonOTNameNotPlayer", waitstate: false },
    SpecialSpec { name: "BufferLottoTicketNumber", waitstate: false },
    SpecialSpec { name: "TryHideBattleTowerReporter", waitstate: false },
    SpecialSpec { name: "DoesPartyHaveEnigmaBerry", waitstate: false },
    SpecialSpec { name: "GenerateContestRand", waitstate: false },
    SpecialSpec { name: "SetChampionSaveWarp", waitstate: false },
    SpecialSpec { name: "TryPutTreasureInvestigatorsOnAir", waitstate: false },
    SpecialSpec { name: "TryPutLotteryWinnerReportOnAir", waitstate: false },
    SpecialSpec { name: "TryPutTrainerFanClubOnAir", waitstate: false },
    SpecialSpec { name: "ShouldHideFanClubInterviewer", waitstate: false },
    SpecialSpec { name: "ShowGlassWorkshopMenu", waitstate: false },
    SpecialSpec { name: "PutFanClubSpecialOnTheAir", waitstate: false },
    SpecialSpec { name: "IncrementDailyPlantedBerries", waitstate: false },
    SpecialSpec { name: "IncrementDailyPickedBerries", waitstate: false },
    SpecialSpec { name: "InitSecretBaseVars", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsSandOrnament", waitstate: false },
    SpecialSpec { name: "DeclinedSecretBaseBattle", waitstate: false },
    SpecialSpec { name: "DrewSecretBaseBattle", waitstate: false },
    SpecialSpec { name: "WonSecretBaseBattle", waitstate: false },
    SpecialSpec { name: "LostSecretBaseBattle", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsDollDecor", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsCushionDecor", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsFurnitureBottom", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsFurnitureMiddle", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsFurnitureTop", waitstate: false },
    SpecialSpec { name: "CheckInteractedWithFriendsPosterDecor", waitstate: false },
    SpecialSpec { name: "SetLilycoveLadyGfx", waitstate: false },
    SpecialSpec { name: "Script_GetLilycoveLadyId", waitstate: false },
    SpecialSpec { name: "GetFavorLadyState", waitstate: false },
    SpecialSpec { name: "BufferFavorLadyRequest", waitstate: false },
    SpecialSpec { name: "HasAnotherPlayerGivenFavorLadyItem", waitstate: false },
    SpecialSpec { name: "BufferFavorLadyItemName", waitstate: false },
    SpecialSpec { name: "BufferFavorLadyPlayerName", waitstate: false },
    SpecialSpec { name: "DidFavorLadyLikeItem", waitstate: false },
    SpecialSpec { name: "Script_FavorLadyOpenBagMenu", waitstate: true },
    SpecialSpec { name: "Script_DoesFavorLadyLikeItem", waitstate: false },
    SpecialSpec { name: "IsFavorLadyThresholdMet", waitstate: false },
    SpecialSpec { name: "FavorLadyGetPrize", waitstate: false },
    SpecialSpec { name: "SetFavorLadyState_Complete", waitstate: false },
    SpecialSpec { name: "GetQuizLadyState", waitstate: false },
    SpecialSpec { name: "GetQuizAuthor", waitstate: false },
    SpecialSpec { name: "IsQuizLadyWaitingForChallenger", waitstate: false },
    SpecialSpec { name: "QuizLadyShowQuizQuestion", waitstate: true },
    SpecialSpec { name: "QuizLadyGetPlayerAnswer", waitstate: true },
    SpecialSpec { name: "IsQuizAnswerCorrect", waitstate: false },
    SpecialSpec { name: "BufferQuizPrizeItem", waitstate: false },
    SpecialSpec { name: "SetQuizLadyState_Complete", waitstate: false },
    SpecialSpec { name: "BufferQuizAuthorNameAndCheckIfLady", waitstate: false },
    SpecialSpec { name: "SetQuizLadyState_GivePrize", waitstate: false },
    SpecialSpec { name: "ClearQuizLadyPlayerAnswer", waitstate: false },
    SpecialSpec { name: "Script_QuizLadyOpenBagMenu", waitstate: true },
    SpecialSpec { name: "ClearQuizLadyQuestionAndAnswer", waitstate: false },
    SpecialSpec { name: "QuizLadySetCustomQuestion", waitstate: true },
    SpecialSpec { name: "QuizLadyTakePrizeForCustomQuiz", waitstate: false },
    SpecialSpec { name: "GetMysteryGiftCardStat", waitstate: false },
    SpecialSpec { name: "QuizLadyRecordCustomQuizData", waitstate: false },
    SpecialSpec { name: "QuizLadySetWaitingForChallenger", waitstate: false },
    SpecialSpec { name: "BufferQuizCorrectAnswer", waitstate: false },
    SpecialSpec { name: "BufferQuizPrizeName", waitstate: false },
    SpecialSpec { name: "QuizLadyPickNewQuestion", waitstate: false },
    SpecialSpec { name: "ShouldContestLadyShowGoOnAir", waitstate: false },
    SpecialSpec { name: "HasPlayerGivenContestLadyPokeblock", waitstate: false },
    SpecialSpec { name: "Script_BufferContestLadyCategoryAndMonName", waitstate: false },
    SpecialSpec { name: "OpenPokeblockCaseForContestLady", waitstate: true },
    SpecialSpec { name: "SetContestLadyGivenPokeblock", waitstate: false },
    SpecialSpec { name: "GetContestLadyMonSpecies", waitstate: false },
    SpecialSpec { name: "GetContestLadyCategory", waitstate: false },
    SpecialSpec { name: "PutLilycoveContestLadyShowOnTheAir", waitstate: false },
    SpecialSpec { name: "CloseBattlePikeCurtain", waitstate: true },
    SpecialSpec { name: "CallApprenticeFunction", waitstate: false },
    SpecialSpec { name: "ShouldTryGetTrainerScript", waitstate: false },
    SpecialSpec { name: "ShowMapNamePopup", waitstate: false },
    SpecialSpec { name: "ShowMapNamePopup", waitstate: false },
    SpecialSpec { name: "DoMirageTowerCeilingCrumble", waitstate: true },
    SpecialSpec { name: "SetMirageTowerVisibility", waitstate: false },
    SpecialSpec { name: "StartPlayerDescendMirageTower", waitstate: true },
    SpecialSpec { name: "BufferTMHMMoveName", waitstate: false },
    SpecialSpec { name: "IsWirelessAdapterConnected", waitstate: false },
    SpecialSpec { name: "TryBecomeLinkLeader", waitstate: true },
    SpecialSpec { name: "TryJoinLinkGroup", waitstate: true },
    SpecialSpec { name: "RunUnionRoom", waitstate: false },
    SpecialSpec { name: "ShowWirelessCommunicationScreen", waitstate: true },
    SpecialSpec { name: "InitUnionRoom", waitstate: false },
    SpecialSpec { name: "BufferUnionRoomPlayerName", waitstate: false },
    SpecialSpec { name: "WonderNews_GetRewardInfo", waitstate: false },
    SpecialSpec { name: "ChooseMonForWirelessMinigame", waitstate: true },
    SpecialSpec { name: "Script_ResetUnionRoomTrade", waitstate: false },
    SpecialSpec { name: "IsBadEggInParty", waitstate: false },
    SpecialSpec { name: "ValidateSavedWonderCard", waitstate: false },
    SpecialSpec { name: "HasAtLeastOneBerry", waitstate: false },
    SpecialSpec { name: "IsPokemonJumpSpeciesInParty", waitstate: false },
    SpecialSpec { name: "ShowPokemonJumpRecords", waitstate: true },
    SpecialSpec { name: "IsDodrioInParty", waitstate: false },
    SpecialSpec { name: "ShowDodrioBerryPickingRecords", waitstate: true },
    SpecialSpec { name: "OffsetCameraForBattle", waitstate: false },
    SpecialSpec { name: "GetDeptStoreDefaultFloorChoice", waitstate: false },
    SpecialSpec { name: "BufferVarsForIVRater", waitstate: false },
    SpecialSpec { name: "LinkContestWaitForConnection", waitstate: true },
    SpecialSpec { name: "GetWirelessCommType", waitstate: false },
    SpecialSpec { name: "LinkContestTryShowWirelessIndicator", waitstate: false },
    SpecialSpec { name: "LinkContestTryHideWirelessIndicator", waitstate: false },
    SpecialSpec { name: "IsWirelessContest", waitstate: false },
    SpecialSpec { name: "ShowRankingHallRecordsWindow", waitstate: false },
    SpecialSpec { name: "ScrollRankingHallRecordsWindow", waitstate: false },
    SpecialSpec { name: "ShowFrontierManiacMessage", waitstate: false },
    SpecialSpec { name: "IsContestWithRSPlayer", waitstate: false },
    SpecialSpec { name: "ClearLinkContestFlags", waitstate: false },
    SpecialSpec { name: "TryContestEModeLinkup", waitstate: true },
    SpecialSpec { name: "ShowScrollableMultichoice", waitstate: true },
    SpecialSpec { name: "ScrollableMultichoice_TryReturnToList", waitstate: false },
    SpecialSpec { name: "BufferBattleTowerElevatorFloors", waitstate: false },
    SpecialSpec { name: "TryStoreHeldItemsInPyramidBag", waitstate: false },
    SpecialSpec { name: "ChooseItemsToTossFromPyramidBag", waitstate: true },
    SpecialSpec { name: "DoBattlePyramidMonsHaveHeldItem", waitstate: false },
    SpecialSpec { name: "BattlePyramidChooseMonHeldItems", waitstate: true },
    SpecialSpec { name: "SetBattleTowerLinkPlayerGfx", waitstate: false },
    SpecialSpec { name: "ShowNatureGirlMessage", waitstate: false },
    SpecialSpec { name: "ShowBattlePointsWindow", waitstate: false },
    SpecialSpec { name: "UpdateBattlePointsWindow", waitstate: false },
    SpecialSpec { name: "CloseBattlePointsWindow", waitstate: false },
    SpecialSpec { name: "GiveFrontierBattlePoints", waitstate: false },
    SpecialSpec { name: "TakeFrontierBattlePoints", waitstate: false },
    SpecialSpec { name: "GetFrontierBattlePoints", waitstate: false },
    SpecialSpec { name: "ShowFrontierExchangeCornerItemIconWindow", waitstate: false },
    SpecialSpec { name: "CloseFrontierExchangeCornerItemIconWindow", waitstate: false },
    SpecialSpec { name: "DisplayBerryPowderVendorMenu", waitstate: false },
    SpecialSpec { name: "RemoveBerryPowderVendorMenu", waitstate: false },
    SpecialSpec { name: "HasEnoughBerryPowder", waitstate: false },
    SpecialSpec { name: "TakeBerryPowder", waitstate: false },
    SpecialSpec { name: "PrintPlayerBerryPowderAmount", waitstate: false },
    SpecialSpec { name: "ShowFrontierGamblerLookingMessage", waitstate: false },
    SpecialSpec { name: "ShowFrontierGamblerGoMessage", waitstate: false },
    SpecialSpec { name: "Script_DoRayquazaScene", waitstate: true },
    SpecialSpec { name: "OpenPokenavForTutorial", waitstate: true },
    SpecialSpec { name: "ScriptMenu_CreateStartMenuForPokenavTutorial", waitstate: true },
    SpecialSpec { name: "CountPlayerTrainerStars", waitstate: false },
    SpecialSpec { name: "BufferBattleFrontierTutorMoveName", waitstate: false },
    SpecialSpec { name: "CloseBattleFrontierTutorWindow", waitstate: false },
    SpecialSpec { name: "ScrollableMultichoice_RedrawPersistentMenu", waitstate: false },
    SpecialSpec { name: "ChooseMonForMoveTutor", waitstate: true },
    SpecialSpec { name: "GetBattleFrontierTutorMoveIndex", waitstate: false },
    SpecialSpec { name: "ScrollableMultichoice_ClosePersistentMenu", waitstate: false },
    SpecialSpec { name: "DoDeoxysRockInteraction", waitstate: true },
    SpecialSpec { name: "SetDeoxysRockPalette", waitstate: false },
    SpecialSpec { name: "CreateEnemyEventMon", waitstate: false },
    SpecialSpec { name: "StartMirageTowerDisintegration", waitstate: true },
    SpecialSpec { name: "StartMirageTowerShake", waitstate: true },
    SpecialSpec { name: "StartMirageTowerFossilFallAndSink", waitstate: true },
    SpecialSpec { name: "ChangeBoxPokemonNickname", waitstate: true },
    SpecialSpec { name: "GetPCBoxToSendMon", waitstate: false },
    SpecialSpec { name: "ShouldShowBoxWasFullMessage", waitstate: false },
    SpecialSpec { name: "SetMatchCallRegisteredFlag", waitstate: false },
    SpecialSpec { name: "DoDomeConfetti", waitstate: false },
    SpecialSpec { name: "CreateAbnormalWeatherEvent", waitstate: false },
    SpecialSpec { name: "GetAbnormalWeatherMapNameAndType", waitstate: false },
    SpecialSpec { name: "GetMartEmployeeObjectEventId", waitstate: false },
    SpecialSpec { name: "SaveForBattleTowerLink", waitstate: true },
    SpecialSpec { name: "Unused_SetWeatherSunny", waitstate: false },
    SpecialSpec { name: "SetUnlockedPokedexFlags", waitstate: false },
    SpecialSpec { name: "IsTrainerRegistered", waitstate: false },
    SpecialSpec { name: "ShouldDoBrailleRegicePuzzle", waitstate: false },
    SpecialSpec { name: "EnableNationalPokedex", waitstate: false },
    SpecialSpec { name: "ScriptMenu_CreateLilycoveSSTidalMultichoice", waitstate: true },
    SpecialSpec { name: "GetLilycoveSSTidalSelection", waitstate: false },
    SpecialSpec { name: "TurnOnTVScreen", waitstate: false },
    SpecialSpec { name: "SetMewAboveGrass", waitstate: false },
    SpecialSpec { name: "ShouldDistributeEonTicket", waitstate: false },
    SpecialSpec { name: "LinkRetireStatusWithBattleTowerPartner", waitstate: true },
    SpecialSpec { name: "BattleTowerReconnectLink", waitstate: false },
    SpecialSpec { name: "CallTrainerHillFunction", waitstate: false },
    SpecialSpec { name: "Script_DoRayquazaScene", waitstate: true },
    SpecialSpec { name: "LoopWingFlapSE", waitstate: false },
    SpecialSpec { name: "DestroyMewEmergingGrassSprite", waitstate: false },
    SpecialSpec { name: "ShowBerryCrushRankings", waitstate: true },
    SpecialSpec { name: "TryBufferWaldaPhrase", waitstate: false },
    SpecialSpec { name: "DoWaldaNamingScreen", waitstate: true },
    SpecialSpec { name: "TryGetWallpaperWithWaldaPhrase", waitstate: false },
    SpecialSpec { name: "PlayerNotAtTrainerHillEntrance", waitstate: false },
    SpecialSpec { name: "GetBattlePyramidHint", waitstate: false },
    SpecialSpec { name: "LoadLinkContestPlayerPalettes", waitstate: false },
    SpecialSpec { name: "ShowTrainerHillRecords", waitstate: true },
    SpecialSpec { name: "PlayerFaceTrainerAfterBattle", waitstate: false },
    SpecialSpec { name: "ResetHealLocationFromDewford", waitstate: false },
    SpecialSpec { name: "IsLastMonThatKnowsSurf", waitstate: false },
    SpecialSpec { name: "CountPartyAliveNonEggMons", waitstate: false },
    SpecialSpec { name: "TryPrepareSecondApproachingTrainer", waitstate: false },
    SpecialSpec { name: "RemoveRecordsWindow", waitstate: false },
    SpecialSpec { name: "CloseDeptStoreElevatorWindow", waitstate: false },
    SpecialSpec { name: "TrySetBattleTowerLinkType", waitstate: false },
];

/// A validated index into [`SPECIALS`]/[`SPECIAL_TABLE`] — the Rust
/// equivalent of a resolved `SPECIAL_<name>` constant
/// (`asm/macros/event.inc`'s `def_special` macro numbers them in table
/// order, `0..SPECIAL_COUNT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpecialId(u16);

impl SpecialId {
    /// Classify a raw `special`/`specialvar` index operand, mirroring the
    /// bounds a valid `SPECIAL_<name>` constant is always within. Upstream's
    /// `ScrCmd_special`/`ScrCmd_specialvar` (`src/scrcmd.c`) never check this
    /// — `gSpecials[index]()` on an out-of-range `index` is an out-of-bounds
    /// function-pointer call (undefined behaviour); this port refuses it
    /// instead, see `CommandTrap::InvalidSpecial`
    /// (`crate::script::commands`).
    #[must_use]
    pub const fn from_index(index: u16) -> Option<Self> {
        if (index as usize) < SPECIAL_COUNT {
            Some(Self(index))
        } else {
            None
        }
    }

    /// This id's index into [`SPECIALS`]/[`SPECIAL_TABLE`] — the inverse of
    /// [`from_index`](Self::from_index).
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }

    /// This id's [`SpecialSpec`] (name + `waitstate` flag).
    #[must_use]
    pub fn spec(self) -> SpecialSpec {
        SPECIALS[usize::from(self.0)]
    }
}

/// A `gSpecials` slot's behavior once implemented: the special's `u16`
/// return value. `ScrCmd_specialvar` writes it to the output var;
/// `ScrCmd_special` calls the same shape of function and discards the
/// result, matching upstream exactly (both opcodes share one `gSpecials`
/// table upstream, so one Rust fn type serves both call sites here too).
pub type SpecialFn = fn(&mut crate::script::commands::ScriptHost) -> u16;

/// Mirrors `gSpecials`: one dispatch slot per [`SpecialId`]. `None` in every
/// slot in this slice — see the module docs for why — so `special`/
/// `specialvar` (`crate::script::commands`) trap
/// `CommandTrap::UnimplementedSpecial` for every currently-valid id. A later
/// slice replaces specific slots with `Some(fn)` as each special's subsystem
/// lands, the same way `COMMAND_TABLE` grows opcode by opcode.
pub const SPECIAL_TABLE: [Option<SpecialFn>; SPECIAL_COUNT] = [None; SPECIAL_COUNT];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specials_table_has_upstreams_exact_entry_count() {
        assert_eq!(SPECIALS.len(), 527);
        assert_eq!(SPECIAL_TABLE.len(), 527);
    }

    // Ground-truth spot checks, transcribed independently from
    // `data/specials.inc`, not derived from this module's own array.
    #[test]
    fn first_and_last_entries_match_upstream_order() {
        assert_eq!(
            SPECIALS[0],
            SpecialSpec {
                name: "HealPlayerParty",
                waitstate: false
            }
        );
        assert_eq!(
            SPECIALS[1],
            SpecialSpec {
                name: "SetCableClubWarp",
                waitstate: false
            }
        );
        assert_eq!(
            SPECIALS[2],
            SpecialSpec {
                name: "DoCableClubWarp",
                waitstate: true
            }
        );
        assert_eq!(
            SPECIALS[SPECIAL_COUNT - 1],
            SpecialSpec {
                name: "TrySetBattleTowerLinkType",
                waitstate: false
            }
        );
    }

    #[test]
    fn waitstate_flag_count_matches_upstream() {
        // `grep -c waitstate=1 data/specials.inc` == 99.
        assert_eq!(SPECIALS.iter().filter(|s| s.waitstate).count(), 99);
    }

    #[test]
    fn from_index_round_trips_within_range() {
        let id = SpecialId::from_index(0).unwrap();
        assert_eq!(id.index(), 0);
        assert_eq!(id.spec().name, "HealPlayerParty");

        let last = SpecialId::from_index(u16::try_from(SPECIAL_COUNT - 1).unwrap()).unwrap();
        assert_eq!(last.spec().name, "TrySetBattleTowerLinkType");
    }

    #[test]
    fn from_index_rejects_out_of_range_indices() {
        assert_eq!(
            SpecialId::from_index(u16::try_from(SPECIAL_COUNT).unwrap()),
            None
        );
        assert_eq!(SpecialId::from_index(u16::MAX), None);
    }

    #[test]
    fn every_special_table_slot_is_unimplemented_in_this_slice() {
        assert!(
            SPECIAL_TABLE.iter().all(Option::is_none),
            "no gSpecials subsystem is wired yet"
        );
    }
}
