//! Blaze component and command ids, with names for logging.
//!
//! Ids and names are taken from Pocket Relay Server
//! (`src/utils/components.rs`, MIT, Copyright (c) 2022-2025 Jacobtread).

#![allow(dead_code)]

/// Component id in the high 16 bits, command id in the low 16 bits.
pub type ComponentKey = u32;

/// Returns the key combining `component` and `command`.
pub const fn component_key(component: u16, command: u16) -> ComponentKey {
    ((component as u32) << 16) + command as u32
}

static COMPONENT_NAMES: &[(u16, &str)] = &[
    (authentication::COMPONENT, "Authentication"),
    (game_manager::COMPONENT, "GameManager"),
    (redirector::COMPONENT, "Redirector"),
    (stats::COMPONENT, "Stats"),
    (util::COMPONENT, "Util"),
    (messaging::COMPONENT, "Messaging"),
    (association_lists::COMPONENT, "AssociationLists"),
    (game_reporting::COMPONENT, "GameReporting"),
    (user_sessions::COMPONENT, "UserSessions"),
];

#[rustfmt::skip]
static COMMAND_NAMES: &[(ComponentKey, &str)] = &[
        // Authentication
        (component_key(authentication::COMPONENT, authentication::CREATE_ACCOUNT), "CreateAccount"),
        (component_key(authentication::COMPONENT, authentication::UPDATE_ACCOUNT), "UpdateAccount"),
        (component_key(authentication::COMPONENT, authentication::UPDATE_PARENTAL_EMAIL), "UpdateParentalEmail"),
        (component_key(authentication::COMPONENT, authentication::LIST_USER_ENTITLEMENTS_2), "ListUserEntitlements2"),
        (component_key(authentication::COMPONENT, authentication::GET_ACCOUNT), "GetAccount"),
        (component_key(authentication::COMPONENT, authentication::GRANT_ENTITLEMENT), "GrantEntitlement"),
        (component_key(authentication::COMPONENT, authentication::LIST_ENTITLEMENTS), "ListEntitlements"),
        (component_key(authentication::COMPONENT, authentication::HAS_ENTITLEMENT), "HasEntitlement"),
        (component_key(authentication::COMPONENT, authentication::GET_USE_COUNT), "GetUseCount"),
        (component_key(authentication::COMPONENT, authentication::DECREMENT_USE_COUNT), "DecrementUseCount"),
        (component_key(authentication::COMPONENT, authentication::GET_AUTH_TOKEN), "GetAuthToken"),
        (component_key(authentication::COMPONENT, authentication::GET_HANDOFF_TOKEN), "GetHandoffToken"),
        (component_key(authentication::COMPONENT, authentication::GET_PASSWORD_RULES), "GetPasswordRules"),
        (component_key(authentication::COMPONENT, authentication::GRANT_ENTITLEMENT_2), "GrantEntitlement2"),
        (component_key(authentication::COMPONENT, authentication::LOGIN), "Login"),
        (component_key(authentication::COMPONENT, authentication::ACCEPT_TOS), "AcceptTOS"),
        (component_key(authentication::COMPONENT, authentication::GET_TOS_INFO), "GetTOSInfo"),
        (component_key(authentication::COMPONENT, authentication::MODIFY_ENTITLEMENT_2), "ModifyEntitlement2"),
        (component_key(authentication::COMPONENT, authentication::CONSUME_CODE), "ConsumeCode"),
        (component_key(authentication::COMPONENT, authentication::PASSWORD_FORGOT), "PasswordForgot"),
        (component_key(authentication::COMPONENT, authentication::GET_TOS_CONTENT), "GetTOSContent"),
        (component_key(authentication::COMPONENT, authentication::GET_PRIVACY_POLICY_CONTENT), "GetPrivacyPolicyContent"),
        (component_key(authentication::COMPONENT, authentication::LIST_PERSONAL_ENTITLEMENTS_2), "ListPersonalEntitlements2"),
        (component_key(authentication::COMPONENT, authentication::SILENT_LOGIN), "SilentLogin"),
        (component_key(authentication::COMPONENT, authentication::CHECK_AGE_REQUIREMENT), "CheckAgeRequirement"),
        (component_key(authentication::COMPONENT, authentication::GET_OPT_IN), "GetOptIn"),
        (component_key(authentication::COMPONENT, authentication::ENABLE_OPT_IN), "EnableOptIn"),
        (component_key(authentication::COMPONENT, authentication::DISABLE_OPT_IN), "DisableOptIn"),
        (component_key(authentication::COMPONENT, authentication::EXPRESS_LOGIN), "ExpressLogin"),
        (component_key(authentication::COMPONENT, authentication::LOGOUT), "Logout"),
        (component_key(authentication::COMPONENT, authentication::CREATE_PERSONA), "CreatePersona"),
        (component_key(authentication::COMPONENT, authentication::GET_PERSONA), "GetPersona"),
        (component_key(authentication::COMPONENT, authentication::LIST_PERSONAS), "ListPersonas"),
        (component_key(authentication::COMPONENT, authentication::LOGIN_PERSONA), "LoginPersona"),
        (component_key(authentication::COMPONENT, authentication::LOGOUT_PERSONA), "LogoutPersona"),
        (component_key(authentication::COMPONENT, authentication::DELETE_PERSONA), "DeletePersona"),
        (component_key(authentication::COMPONENT, authentication::DISABLE_PERSONA), "DisablePersona"),
        (component_key(authentication::COMPONENT, authentication::LIST_DEVICE_ACCOUNTS), "ListDeviceAccounts"),
        (component_key(authentication::COMPONENT, authentication::XBOX_CREATE_ACCOUNT), "XboxCreateAccount"),
        (component_key(authentication::COMPONENT, authentication::ORIGIN_LOGIN), "OriginLogin"),
        (component_key(authentication::COMPONENT, authentication::XBOX_ASSOCIATE_ACCOUNT), "XboxAssociateAccount"),
        (component_key(authentication::COMPONENT, authentication::XBOX_LOGIN), "XboxLogin"),
        (component_key(authentication::COMPONENT, authentication::PS3_CREATE_ACCOUNT), "PS3CreateAccount"),
        (component_key(authentication::COMPONENT, authentication::PS3_ASSOCIATE_ACCOUNT), "PS3AssociateAccount"),
        (component_key(authentication::COMPONENT, authentication::PS3_LOGIN), "PS3Login"),
        (component_key(authentication::COMPONENT, authentication::VALIDATE_SESSION_KEY), "ValidateSessionKey"),
        (component_key(authentication::COMPONENT, authentication::CREATE_WAL_USER_SESSION), "CreateWalUserSession"),
        (component_key(authentication::COMPONENT, authentication::ACCEPT_LEGAL_DOCS), "AcceptLegalDocs"),
        (component_key(authentication::COMPONENT, authentication::GET_LEGAL_DOCS_INFO), "GetLegalDocsInfo"),
        (component_key(authentication::COMPONENT, authentication::GET_TERMS_OF_SERVICE_CONTENT), "GetTermsOfServiceContent"),
        (component_key(authentication::COMPONENT, authentication::DEVICE_LOGIN_GUEST), "DeviceLoginGuest"),

        // Game Manager
        (component_key(game_manager::COMPONENT, game_manager::CREATE_GAME), "CreateGame"),
        (component_key(game_manager::COMPONENT, game_manager::DESTROY_GAME), "DestroyGame"),
        (component_key(game_manager::COMPONENT, game_manager::ADVANCE_GAME_STATE), "AdvanceGameState"),
        (component_key(game_manager::COMPONENT, game_manager::SET_GAME_SETTINGS), "SetGameSettings"),
        (component_key(game_manager::COMPONENT, game_manager::SET_PLAYER_CAPACITY), "SetPlayerCapacity"),
        (component_key(game_manager::COMPONENT, game_manager::SET_PRESENCE_MODE), "SetPresenceMode"),
        (component_key(game_manager::COMPONENT, game_manager::SET_GAME_ATTRIBUTES), "SetGameAttributes"),
        (component_key(game_manager::COMPONENT, game_manager::SET_PLAYER_ATTRIBUTES), "SetPlayerAttributes"),
        (component_key(game_manager::COMPONENT, game_manager::JOIN_GAME), "JoinGame"),
        (component_key(game_manager::COMPONENT, game_manager::REMOVE_PLAYER), "RemovePlayer"),
        (component_key(game_manager::COMPONENT, game_manager::START_MATCHMAKING), "StartMatchmaking"),
        (component_key(game_manager::COMPONENT, game_manager::CANCEL_MATCHMAKING), "CancelMatchmaking"),
        (component_key(game_manager::COMPONENT, game_manager::FINALIZE_GAME_CREATION), "FinalizeGameCreation"),
        (component_key(game_manager::COMPONENT, game_manager::LIST_GAMES), "ListGames"),
        (component_key(game_manager::COMPONENT, game_manager::SET_PLAYER_CUSTOM_DATA), "SetPlayerCustomData"),
        (component_key(game_manager::COMPONENT, game_manager::REPLAY_GAME), "ReplayGame"),
        (component_key(game_manager::COMPONENT, game_manager::RETURN_DEDICATED_SERVER_TO_POOL), "ReturnDedicatedServerToPool"),
        (component_key(game_manager::COMPONENT, game_manager::JOIN_GAME_BY_GROUP), "JoinGameByGroup"),
        (component_key(game_manager::COMPONENT, game_manager::LEAVE_GAME_BY_GROUP), "LeaveGameByGroup"),
        (component_key(game_manager::COMPONENT, game_manager::MIGRATE_GAME), "MigrateGame"),
        (component_key(game_manager::COMPONENT, game_manager::UPDATE_GAME_HOST_MIGRATION_STATUS), "UpdateGameHostMigrationStatus"),
        (component_key(game_manager::COMPONENT, game_manager::RESET_DEDICATED_SERVER), "ResetDedicatedServer"),
        (component_key(game_manager::COMPONENT, game_manager::UPDATE_GAME_SESSION), "UpdateGameSession"),
        (component_key(game_manager::COMPONENT, game_manager::BAN_PLAYER), "BanPlayer"),
        (component_key(game_manager::COMPONENT, game_manager::UPDATE_MESH_CONNECTION), "UpdateMeshConnection"),
        (component_key(game_manager::COMPONENT, game_manager::REMOVE_PLAYER_FROM_BANNED_LIST), "RemovePlayerFromBannedList"),
        (component_key(game_manager::COMPONENT, game_manager::CLEAR_BANNED_LIST), "ClearBannedList"),
        (component_key(game_manager::COMPONENT, game_manager::GET_BANNED_LIST), "GetBannedList"),
        (component_key(game_manager::COMPONENT, game_manager::ADD_QUEUED_PLAYER_TO_GAME), "AddQueuedPlayerToGame"),
        (component_key(game_manager::COMPONENT, game_manager::UPDATE_GAME_NAME), "UpdateGameName"),
        (component_key(game_manager::COMPONENT, game_manager::EJECT_HOST), "EjectHost"),
        (component_key(game_manager::COMPONENT, game_manager::GET_GAME_LIST_SNAPSHOT), "GetGameListSnapshot"),
        (component_key(game_manager::COMPONENT, game_manager::GET_GAME_LIST_SUBSCRIPTION), "GetGameListSubscription"),
        (component_key(game_manager::COMPONENT, game_manager::DESTROY_GAME_LIST), "DestroyGameList"),
        (component_key(game_manager::COMPONENT, game_manager::GET_FULL_GAME_DATA), "GetFullGameData"),
        (component_key(game_manager::COMPONENT, game_manager::GET_MATCHMAKING_CONFIG), "GetMatchmakingConfig"),
        (component_key(game_manager::COMPONENT, game_manager::GET_GAME_DATA_FROM_ID), "GetGameDataFromID"),
        (component_key(game_manager::COMPONENT, game_manager::ADD_ADMIN_PLAYER), "AddAdminPlayer"),
        (component_key(game_manager::COMPONENT, game_manager::REMOVE_ADMIN_PLAYER), "RemoveAdminPlayer"),
        (component_key(game_manager::COMPONENT, game_manager::SET_PLAYER_TEAM), "SetPlayerTeam"),
        (component_key(game_manager::COMPONENT, game_manager::CHANGE_GAME_TEAM_ID), "ChangeGameTeamID"),
        (component_key(game_manager::COMPONENT, game_manager::MIGRATE_ADMIN_PLAYER), "MigrateAdminPlayer"),
        (component_key(game_manager::COMPONENT, game_manager::GET_USER_SET_GAME_LIST_SUBSCRIPTION), "GetUserSetGameListSubscription"),
        (component_key(game_manager::COMPONENT, game_manager::SWAP_PLAYERS_TEAM), "SwapPlayersTeam"),
        (component_key(game_manager::COMPONENT, game_manager::REGISTER_DYNAMIC_DEDICATED_SERVER_CREATOR), "RegisterDynamicDedicatedServerCreator"),
        (component_key(game_manager::COMPONENT, game_manager::UNREGISTER_DYNAMIC_DEDICATED_SERVER_CREATOR), "UnregisterDynamicDedicatedServerCreator"),

        // Redirector
        (component_key(redirector::COMPONENT, redirector::GET_SERVER_INSTANCE), "GetServerInstance"),

        // Stats
        (component_key(stats::COMPONENT, stats::GET_STAT_DECS), "GetStatDecs"),
        (component_key(stats::COMPONENT, stats::GET_STATS), "GetStats"),
        (component_key(stats::COMPONENT, stats::GET_STAT_GROUP_LIST), "GetStatGroupList"),
        (component_key(stats::COMPONENT, stats::GET_STAT_GROUP), "GetStatGroup"),
        (component_key(stats::COMPONENT, stats::GET_STATS_BY_GROUP), "GetStatsByGroup"),
        (component_key(stats::COMPONENT, stats::GET_DATE_RANGE), "GetDateRange"),
        (component_key(stats::COMPONENT, stats::GET_ENTITY_COUNT), "GetEntityCount"),
        (component_key(stats::COMPONENT, stats::GET_LEADERBOARD_GROUP), "GetLeaderboardGroup"),
        (component_key(stats::COMPONENT, stats::GET_LEADERBOARD_FOLDER_GROUP), "GetLeaderboardFolderGroup"),
        (component_key(stats::COMPONENT, stats::GET_LEADERBOARD), "GetLeaderboard"),
        (component_key(stats::COMPONENT, stats::GET_CENTERED_LEADERBOARD), "GetCenteredLeaderboard"),
        (component_key(stats::COMPONENT, stats::GET_FILTERED_LEADERBOARD), "GetFilteredLeaderboard"),
        (component_key(stats::COMPONENT, stats::GET_KEY_SCOPES_MAP), "GetKeyScopesMap"),
        (component_key(stats::COMPONENT, stats::GET_STATS_BY_GROUP_ASYNC), "GetStatsByGroupASync"),
        (component_key(stats::COMPONENT, stats::GET_LEADERBOARD_TREE_ASYNC), "GetLeaderboardTreeAsync"),
        (component_key(stats::COMPONENT, stats::GET_LEADERBOARD_ENTITY_COUNT), "GetLeaderboardEntityCount"),
        (component_key(stats::COMPONENT, stats::GET_STAT_CATEGORY_LIST), "GetStatCategoryList"),
        (component_key(stats::COMPONENT, stats::GET_PERIOD_IDS), "GetPeriodIDs"),
        (component_key(stats::COMPONENT, stats::GET_LEADERBOARD_RAW), "GetLeaderboardRaw"),
        (component_key(stats::COMPONENT, stats::GET_CENTERED_LEADERBOARD_RAW), "GetCenteredLeaderboardRaw"),
        (component_key(stats::COMPONENT, stats::GET_FILTERED_LEADERBOARD_RAW), "GetFilteredLeaderboardRaw"),
        (component_key(stats::COMPONENT, stats::CHANGE_KEY_SCOPE_VALUE), "ChangeKeyScopeValue"),

        // Util
        (component_key(util::COMPONENT, util::FETCH_CLIENT_CONFIG), "FetchClientConfig"),
        (component_key(util::COMPONENT, util::PING), "Ping"),
        (component_key(util::COMPONENT, util::SET_CLIENT_DATA), "SetClientData"),
        (component_key(util::COMPONENT, util::LOCALIZE_STRINGS), "LocalizeStrings"),
        (component_key(util::COMPONENT, util::GET_TELEMETRY_SERVER), "GetTelemetryServer"),
        (component_key(util::COMPONENT, util::GET_TICKER_SERVER), "GetTickerServer"),
        (component_key(util::COMPONENT, util::PRE_AUTH), "PreAuth"),
        (component_key(util::COMPONENT, util::POST_AUTH), "PostAuth"),
        (component_key(util::COMPONENT, util::USER_SETTINGS_LOAD), "UserSettingsLoad"),
        (component_key(util::COMPONENT, util::USER_SETTINGS_SAVE), "UserSettingsSave"),
        (component_key(util::COMPONENT, util::USER_SETTINGS_LOAD_ALL), "UserSettingsLoadAll"),
        (component_key(util::COMPONENT, util::DELETE_USER_SETTINGS), "DeleteUserSettings"),
        (component_key(util::COMPONENT, util::FILTER_FOR_PROFANITY), "FilterForProfanity"),
        (component_key(util::COMPONENT, util::FETCH_QOS_CONFIG), "FetchQOSConfig"),
        (component_key(util::COMPONENT, util::SET_CLIENT_METRICS), "SetClientMetrics"),
        (component_key(util::COMPONENT, util::SET_CONNECTION_STATE), "SetConnectionState"),
        (component_key(util::COMPONENT, util::GET_PSS_CONFIG), "GetPSSConfig"),
        (component_key(util::COMPONENT, util::GET_USER_OPTIONS), "GetUserOptions"),
        (component_key(util::COMPONENT, util::SET_USER_OPTIONS), "SetUserOptions"),
        (component_key(util::COMPONENT, util::SUSPEND_USER_PING), "SuspendUserPing"),

        // Messaging
        (component_key(messaging::COMPONENT, messaging::FETCH_MESSAGES), "FetchMessages"),
        (component_key(messaging::COMPONENT, messaging::PURGE_MESSAGES), "PurgeMessages"),
        (component_key(messaging::COMPONENT, messaging::TOUCH_MESSAGES), "TouchMessages"),
        (component_key(messaging::COMPONENT, messaging::GET_MESSAGES), "GetMessages"),

        // Association Lists
        (component_key(association_lists::COMPONENT, association_lists::ADD_USERS_TO_LIST), "AddUsersToList"),
        (component_key(association_lists::COMPONENT, association_lists::REMOVE_USERS_FROM_LIST), "RemoveUsersFromList"),
        (component_key(association_lists::COMPONENT, association_lists::CLEAR_LIST), "ClearList"),
        (component_key(association_lists::COMPONENT, association_lists::SET_USERS_TO_LIST), "SetUsersToList"),
        (component_key(association_lists::COMPONENT, association_lists::GET_LIST_FOR_USER), "GetListForUser"),
        (component_key(association_lists::COMPONENT, association_lists::GET_LISTS), "GetLists"),
        (component_key(association_lists::COMPONENT, association_lists::SUBSCRIBE_TO_LISTS), "SubscribeToLists"),
        (component_key(association_lists::COMPONENT, association_lists::UNSUBSCRIBE_TO_LISTS), "UnsubscribeToLists"),
        (component_key(association_lists::COMPONENT, association_lists::GET_CONFIG_LISTS_INFO), "GetConfigListsInfo"),

        // Game Reporting
        (component_key(game_reporting::COMPONENT, game_reporting::SUBMIT_GAME_REPORT), "SubmitGameReport"),
        (component_key(game_reporting::COMPONENT, game_reporting::SUBMIT_OFFLINE_GAME_REPORT), "SubmitOfflineGameReport"),
        (component_key(game_reporting::COMPONENT, game_reporting::SUBMIT_GAME_EVENTS), "SubmitGameEvents"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_QUERY), "GetGameReportQuery"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_QUERIES_LIST), "GetGameReportQueriesList"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORTS), "GetGameReports"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_VIEW), "GetGameReportView"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_VIEW_INFO), "GetGameReportViewInfo"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_VIEW_INFO_LIST), "GetGameReportViewInfoList"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_TYPES), "GetGameReportTypes"),
        (component_key(game_reporting::COMPONENT, game_reporting::UPDATE_METRICS), "UpdateMetrics"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_COLUMN_INFO), "GetGameReportColumnInfo"),
        (component_key(game_reporting::COMPONENT, game_reporting::GET_GAME_REPORT_COLUMN_VALUES), "GetGameReportColumnValues"),
        (component_key(game_reporting::COMPONENT, game_reporting::SUBMIT_TRUSTED_MID_GAME_REPORT), "SubmitTrustedMidGameReport"),
        (component_key(game_reporting::COMPONENT, game_reporting::SUBMIT_TRUSTED_END_GAME_REPORT), "SubmitTrustedEndGameReport"),

        // User Sessions
        (component_key(user_sessions::COMPONENT, user_sessions::UPDATE_HARDWARE_FLAGS), "UpdateHardwareFlags"),
        (component_key(user_sessions::COMPONENT, user_sessions::LOOKUP_USER), "LookupUser"),
        (component_key(user_sessions::COMPONENT, user_sessions::LOOKUP_USERS), "LookupUsers"),
        (component_key(user_sessions::COMPONENT, user_sessions::LOOKUP_USERS_BY_PREFIX), "LookupUsersByPrefix"),
        (component_key(user_sessions::COMPONENT, user_sessions::UPDATE_NETWORK_INFO), "UpdateNetworkInfo"),
        (component_key(user_sessions::COMPONENT, user_sessions::LOOKUP_USER_GEO_IP_DATA), "LookupUserGeoIPData"),
        (component_key(user_sessions::COMPONENT, user_sessions::OVERRIDE_USER_GEO_IP_DATA), "OverrideUserGeoIPData"),
        (component_key(user_sessions::COMPONENT, user_sessions::UPDATE_USER_SESSION_CLIENT_DATA), "UpdateUserSessionClientData"),
        (component_key(user_sessions::COMPONENT, user_sessions::SET_USER_INFO_ATTRIBUTE), "SetUserInfoAttribute"),
        (component_key(user_sessions::COMPONENT, user_sessions::RESET_USER_GEO_IP_DATA), "ResetUserGeoIPData"),
        (component_key(user_sessions::COMPONENT, user_sessions::LOOKUP_USER_SESSION_ID), "LookupUserSessionID"),
        (component_key(user_sessions::COMPONENT, user_sessions::FETCH_LAST_LOCALE_USED_AND_AUTH_ERROR), "FetchLastLocaleUsedAndAuthError"),
        (component_key(user_sessions::COMPONENT, user_sessions::FETCH_USER_FIRST_LAST_AUTH_TIME), "FetchUserFirstLastAuthTime"),
        (component_key(user_sessions::COMPONENT, user_sessions::RESUME_SESSION), "ResumeSession"),
    ];

#[rustfmt::skip]
static NOTIFICATION_NAMES: &[(ComponentKey, &str)] = &[
        // Game Manager
        (component_key(game_manager::COMPONENT, game_manager::MATCHMAKING_FAILED), "MatchmakingFailed"),
        (component_key(game_manager::COMPONENT, game_manager::MATCHMAKING_ASYNC_STATUS), "MatchmakingAsyncStatus"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_CREATED), "GameCreated"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_REMOVED), "GameRemoved"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_SETUP), "GameSetup"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_JOINING), "PlayerJoining"),
        (component_key(game_manager::COMPONENT, game_manager::JOINING_PLAYER_INITIATE_CONNECTIONS), "JoiningPlayerInitiateConnections"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_JOINING_QUEUE), "PlayerJoiningQueue"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_PROMOTED_FROM_QUEUE), "PlayerPromotedFromQueue"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_CLAIMING_RESERVATION), "PlayerClaimingReservation"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_JOIN_COMPLETED), "PlayerJoinCompleted"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_REMOVED), "PlayerRemoved"),
        (component_key(game_manager::COMPONENT, game_manager::HOST_MIGRATION_FINISHED), "HostMigrationFinished"),
        (component_key(game_manager::COMPONENT, game_manager::HOST_MIGRATION_START), "HostMigrationStart"),
        (component_key(game_manager::COMPONENT, game_manager::PLATFORM_HOST_INITIALIZED), "PlatformHostInitialized"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_ATTRIB_CHANGE), "GameAttribChange"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_ATTRIB_CHANGE), "PlayerAttribChange"),
        (component_key(game_manager::COMPONENT, game_manager::PLAYER_CUSTOM_DATA_CHANGE), "PlayerCustomDataChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_STATE_CHANGE), "GameStateChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_SETTINGS_CHANGE), "GameSettingsChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_CAPACITY_CHANGE), "GameCapacityChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_RESET), "GameReset"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_REPORTING_ID_CHANGE), "GameReportingIDChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_SESSION_UPDATED), "GameSessionUpdated"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_PLAYER_STATE_CHANGE), "GamePlayerStateChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_PLAYER_TEAM_CHANGE), "GamePlayerTeamChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_TEAM_ID_CHANGE), "GameTeamIDChange"),
        (component_key(game_manager::COMPONENT, game_manager::PROCESS_QUEUE), "PROCESS_QUEUE"),
        (component_key(game_manager::COMPONENT, game_manager::PRESENCE_MODE_CHANGED), "PresenceModeChanged"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_PLAYER_QUEUE_POSITION_CHANGE), "GamePlayerQueuePositionChange"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_LIST_UPDATE), "GameListUpdate"),
        (component_key(game_manager::COMPONENT, game_manager::ADMIN_LIST_CHANGE), "AdminListChange"),
        (component_key(game_manager::COMPONENT, game_manager::CREATE_DYNAMIC_DEDICATED_SERVER_GAME), "CreateDynamicDedicatedServerGame"),
        (component_key(game_manager::COMPONENT, game_manager::GAME_NAME_CHANGE), "GameNameChange"),

        // Messaging
        (component_key(messaging::COMPONENT, messaging::SEND_MESSAGE), "SendMessage"),

        // Game Reporting
        (component_key(game_reporting::COMPONENT, game_reporting::GAME_REPORT_SUBMITTED), "GameReportSubmitted"),

        // User Sessions
        (component_key(user_sessions::COMPONENT, user_sessions::USER_SESSION_EXTENDED_DATA_UPDATE), "UserSessionExtendedDataUpdate"),
        (component_key(user_sessions::COMPONENT, user_sessions::USER_ADDED), "UserAdded"),
        (component_key(user_sessions::COMPONENT, user_sessions::USER_UPDATED), "UserUpdated"),
        (component_key(user_sessions::COMPONENT, user_sessions::USER_REMOVED), "UserRemoved"),
        (component_key(user_sessions::COMPONENT, user_sessions::USER_SESSION_DISCONNECTED), "UserSessionDisconnected"),
        (component_key(user_sessions::COMPONENT, user_sessions::USER_AUTHENTICATED), "UserAuthenticated"),
        (component_key(user_sessions::COMPONENT, user_sessions::USER_UNAUTHENTICATED), "UserUnauthenticated"),
    ];

/// Returns the name of `component`, if known.
pub fn component_name(component: u16) -> Option<&'static str> {
    COMPONENT_NAMES
        .iter()
        .find(|(c, _)| *c == component)
        .map(|(_, n)| *n)
}

/// Returns the name of a command, or of a notification when `notify`, if known.
pub fn command_name(component: u16, command: u16, notify: bool) -> Option<&'static str> {
    let key = component_key(component, command);
    let table = if notify {
        NOTIFICATION_NAMES
    } else {
        COMMAND_NAMES
    };
    table.iter().find(|(k, _)| *k == key).map(|(_, n)| *n)
}

pub mod authentication {
    pub const COMPONENT: u16 = 0x1;

    // Commands
    pub const CREATE_ACCOUNT: u16 = 0xA;
    pub const UPDATE_ACCOUNT: u16 = 0x14;
    pub const UPDATE_PARENTAL_EMAIL: u16 = 0x1C;
    pub const LIST_USER_ENTITLEMENTS_2: u16 = 0x1D;
    pub const GET_ACCOUNT: u16 = 0x1E;
    pub const GRANT_ENTITLEMENT: u16 = 0x1F;
    pub const LIST_ENTITLEMENTS: u16 = 0x20;
    pub const HAS_ENTITLEMENT: u16 = 0x21;
    pub const GET_USE_COUNT: u16 = 0x22;
    pub const DECREMENT_USE_COUNT: u16 = 0x23;
    pub const GET_AUTH_TOKEN: u16 = 0x24;
    pub const GET_HANDOFF_TOKEN: u16 = 0x25;
    pub const GET_PASSWORD_RULES: u16 = 0x26;
    pub const GRANT_ENTITLEMENT_2: u16 = 0x27;
    pub const LOGIN: u16 = 0x28;
    pub const ACCEPT_TOS: u16 = 0x29;
    pub const GET_TOS_INFO: u16 = 0x2A;
    pub const MODIFY_ENTITLEMENT_2: u16 = 0x2B;
    pub const CONSUME_CODE: u16 = 0x2C;
    pub const PASSWORD_FORGOT: u16 = 0x2D;
    pub const GET_TOS_CONTENT: u16 = 0x2E;
    pub const GET_PRIVACY_POLICY_CONTENT: u16 = 0x2F;
    pub const LIST_PERSONAL_ENTITLEMENTS_2: u16 = 0x30;
    pub const SILENT_LOGIN: u16 = 0x32;
    pub const CHECK_AGE_REQUIREMENT: u16 = 0x33;
    pub const GET_OPT_IN: u16 = 0x34;
    pub const ENABLE_OPT_IN: u16 = 0x35;
    pub const DISABLE_OPT_IN: u16 = 0x36;
    pub const EXPRESS_LOGIN: u16 = 0x3C;
    pub const LOGOUT: u16 = 0x46;
    pub const CREATE_PERSONA: u16 = 0x50;
    pub const GET_PERSONA: u16 = 0x5A;
    pub const LIST_PERSONAS: u16 = 0x64;
    pub const LOGIN_PERSONA: u16 = 0x6E;
    pub const LOGOUT_PERSONA: u16 = 0x78;
    pub const DELETE_PERSONA: u16 = 0x8C;
    pub const DISABLE_PERSONA: u16 = 0x8D;
    pub const LIST_DEVICE_ACCOUNTS: u16 = 0x8F;
    pub const XBOX_CREATE_ACCOUNT: u16 = 0x96;
    pub const ORIGIN_LOGIN: u16 = 0x98;
    pub const XBOX_ASSOCIATE_ACCOUNT: u16 = 0xA0;
    pub const XBOX_LOGIN: u16 = 0xAA;
    pub const PS3_CREATE_ACCOUNT: u16 = 0xB4;
    pub const PS3_ASSOCIATE_ACCOUNT: u16 = 0xBE;
    pub const PS3_LOGIN: u16 = 0xC8;
    pub const VALIDATE_SESSION_KEY: u16 = 0xD2;
    pub const CREATE_WAL_USER_SESSION: u16 = 0xE6;
    pub const ACCEPT_LEGAL_DOCS: u16 = 0xF1;
    pub const GET_LEGAL_DOCS_INFO: u16 = 0xF2;
    pub const GET_TERMS_OF_SERVICE_CONTENT: u16 = 0xF6;
    pub const DEVICE_LOGIN_GUEST: u16 = 0x12C;
}

pub mod game_manager {
    use tdf::ObjectType;

    pub const COMPONENT: u16 = 0x4;

    // Components
    pub const CREATE_GAME: u16 = 0x1;
    pub const DESTROY_GAME: u16 = 0x2;
    pub const ADVANCE_GAME_STATE: u16 = 0x3;
    pub const SET_GAME_SETTINGS: u16 = 0x4;
    pub const SET_PLAYER_CAPACITY: u16 = 0x5;
    pub const SET_PRESENCE_MODE: u16 = 0x6;
    pub const SET_GAME_ATTRIBUTES: u16 = 0x7;
    pub const SET_PLAYER_ATTRIBUTES: u16 = 0x8;
    pub const JOIN_GAME: u16 = 0x9;
    // 0xA --
    pub const REMOVE_PLAYER: u16 = 0xB;
    // 0xC --
    pub const START_MATCHMAKING: u16 = 0xD;
    pub const CANCEL_MATCHMAKING: u16 = 0xE;
    pub const FINALIZE_GAME_CREATION: u16 = 0xF;
    // 0x10 --
    pub const LIST_GAMES: u16 = 0x11;
    pub const SET_PLAYER_CUSTOM_DATA: u16 = 0x12;
    pub const REPLAY_GAME: u16 = 0x13;
    pub const RETURN_DEDICATED_SERVER_TO_POOL: u16 = 0x14;
    pub const JOIN_GAME_BY_GROUP: u16 = 0x15;
    pub const LEAVE_GAME_BY_GROUP: u16 = 0x16;
    pub const MIGRATE_GAME: u16 = 0x17;
    pub const UPDATE_GAME_HOST_MIGRATION_STATUS: u16 = 0x18;
    pub const RESET_DEDICATED_SERVER: u16 = 0x19;
    pub const UPDATE_GAME_SESSION: u16 = 0x1A;
    pub const BAN_PLAYER: u16 = 0x1B;
    // 0x1C --
    pub const UPDATE_MESH_CONNECTION: u16 = 0x1D;
    // 0x1E --
    pub const REMOVE_PLAYER_FROM_BANNED_LIST: u16 = 0x1F;
    pub const CLEAR_BANNED_LIST: u16 = 0x20;
    pub const GET_BANNED_LIST: u16 = 0x21;
    // 0x22-0x25 --
    pub const ADD_QUEUED_PLAYER_TO_GAME: u16 = 0x26;
    pub const UPDATE_GAME_NAME: u16 = 0x27;
    pub const EJECT_HOST: u16 = 0x28;
    // 0x29-0x63 --
    pub const GET_GAME_LIST_SNAPSHOT: u16 = 0x64;
    pub const GET_GAME_LIST_SUBSCRIPTION: u16 = 0x65;
    pub const DESTROY_GAME_LIST: u16 = 0x66;
    pub const GET_FULL_GAME_DATA: u16 = 0x67;
    pub const GET_MATCHMAKING_CONFIG: u16 = 0x68;
    pub const GET_GAME_DATA_FROM_ID: u16 = 0x69;
    pub const ADD_ADMIN_PLAYER: u16 = 0x6A;
    pub const REMOVE_ADMIN_PLAYER: u16 = 0x6B;
    pub const SET_PLAYER_TEAM: u16 = 0x6C;
    pub const CHANGE_GAME_TEAM_ID: u16 = 0x6D;
    pub const MIGRATE_ADMIN_PLAYER: u16 = 0x6E;
    pub const GET_USER_SET_GAME_LIST_SUBSCRIPTION: u16 = 0x6F;
    pub const SWAP_PLAYERS_TEAM: u16 = 0x70;
    // 0x71-0x95 --
    pub const REGISTER_DYNAMIC_DEDICATED_SERVER_CREATOR: u16 = 0x96;
    pub const UNREGISTER_DYNAMIC_DEDICATED_SERVER_CREATOR: u16 = 0x97;

    // Notifications
    pub const MATCHMAKING_FAILED: u16 = 0xA;
    // 0xB --
    pub const MATCHMAKING_ASYNC_STATUS: u16 = 0xC;
    // 0xD-0xE --
    pub const GAME_CREATED: u16 = 0xF;
    pub const GAME_REMOVED: u16 = 0x10;
    // 0x11-0x13 --
    pub const GAME_SETUP: u16 = 0x14;
    pub const PLAYER_JOINING: u16 = 0x15;
    pub const JOINING_PLAYER_INITIATE_CONNECTIONS: u16 = 0x16;
    pub const PLAYER_JOINING_QUEUE: u16 = 0x17;
    pub const PLAYER_PROMOTED_FROM_QUEUE: u16 = 0x18;
    pub const PLAYER_CLAIMING_RESERVATION: u16 = 0x19;
    pub const PLAYER_JOIN_COMPLETED: u16 = 0x1E;
    // 0x1F-0x27 --
    pub const PLAYER_REMOVED: u16 = 0x28;
    // 0x29-0x3B --
    pub const HOST_MIGRATION_FINISHED: u16 = 0x3C;
    // 0x3D-0x45 --
    pub const HOST_MIGRATION_START: u16 = 0x46;
    pub const PLATFORM_HOST_INITIALIZED: u16 = 0x47;
    // 0x48-0x4F --
    pub const GAME_ATTRIB_CHANGE: u16 = 0x50;
    // 0x51-0x59 --
    pub const PLAYER_ATTRIB_CHANGE: u16 = 0x5A;
    pub const PLAYER_CUSTOM_DATA_CHANGE: u16 = 0x5F;
    // 0x60-0x63 --
    pub const GAME_STATE_CHANGE: u16 = 0x64;
    // 0x64-0x6D --
    pub const GAME_SETTINGS_CHANGE: u16 = 0x6E;
    pub const GAME_CAPACITY_CHANGE: u16 = 0x6F;
    pub const GAME_RESET: u16 = 0x70;
    pub const GAME_REPORTING_ID_CHANGE: u16 = 0x71;
    // 0x72 --
    pub const GAME_SESSION_UPDATED: u16 = 0x73;
    pub const GAME_PLAYER_STATE_CHANGE: u16 = 0x74;
    pub const GAME_PLAYER_TEAM_CHANGE: u16 = 0x75;
    pub const GAME_TEAM_ID_CHANGE: u16 = 0x76;
    pub const PROCESS_QUEUE: u16 = 0x77;
    pub const PRESENCE_MODE_CHANGED: u16 = 0x78;
    pub const GAME_PLAYER_QUEUE_POSITION_CHANGE: u16 = 0x79;
    // 0x7A-0xC8 --
    pub const GAME_LIST_UPDATE: u16 = 0xC9;
    pub const ADMIN_LIST_CHANGE: u16 = 0xCA;
    // 0xCB-0xDB --
    pub const CREATE_DYNAMIC_DEDICATED_SERVER_GAME: u16 = 0xDC;
    // 0xDD-0xE5 --
    pub const GAME_NAME_CHANGE: u16 = 0xE6;

    // Object Types
    pub const GAME_TYPE: ObjectType = ObjectType::new(COMPONENT, 1);
}

pub mod redirector {
    pub const COMPONENT: u16 = 0x5;

    pub const GET_SERVER_INSTANCE: u16 = 0x1;
}

pub mod stats {
    pub const COMPONENT: u16 = 0x7;

    /// Components
    pub const GET_STAT_DECS: u16 = 0x1;
    pub const GET_STATS: u16 = 0x2;
    pub const GET_STAT_GROUP_LIST: u16 = 0x3;
    pub const GET_STAT_GROUP: u16 = 0x4;
    pub const GET_STATS_BY_GROUP: u16 = 0x5;
    pub const GET_DATE_RANGE: u16 = 0x6;
    pub const GET_ENTITY_COUNT: u16 = 0x7;
    // 0x8-0x9 --
    pub const GET_LEADERBOARD_GROUP: u16 = 0xA;
    pub const GET_LEADERBOARD_FOLDER_GROUP: u16 = 0xB;
    pub const GET_LEADERBOARD: u16 = 0xC;
    pub const GET_CENTERED_LEADERBOARD: u16 = 0xD;
    pub const GET_FILTERED_LEADERBOARD: u16 = 0xE;
    pub const GET_KEY_SCOPES_MAP: u16 = 0xF;
    pub const GET_STATS_BY_GROUP_ASYNC: u16 = 0x10;
    pub const GET_LEADERBOARD_TREE_ASYNC: u16 = 0x11;
    pub const GET_LEADERBOARD_ENTITY_COUNT: u16 = 0x12;
    pub const GET_STAT_CATEGORY_LIST: u16 = 0x13;
    pub const GET_PERIOD_IDS: u16 = 0x14;
    pub const GET_LEADERBOARD_RAW: u16 = 0x15;
    pub const GET_CENTERED_LEADERBOARD_RAW: u16 = 0x16;
    pub const GET_FILTERED_LEADERBOARD_RAW: u16 = 0x17;
    pub const CHANGE_KEY_SCOPE_VALUE: u16 = 0x18;
}

pub mod util {
    pub const COMPONENT: u16 = 0x9;

    /// Components
    pub const FETCH_CLIENT_CONFIG: u16 = 0x1;
    pub const PING: u16 = 0x2;
    pub const SET_CLIENT_DATA: u16 = 0x3;
    pub const LOCALIZE_STRINGS: u16 = 0x4;
    pub const GET_TELEMETRY_SERVER: u16 = 0x5;
    pub const GET_TICKER_SERVER: u16 = 0x6;
    pub const PRE_AUTH: u16 = 0x7;
    pub const POST_AUTH: u16 = 0x8;
    // 0x9 --
    pub const USER_SETTINGS_LOAD: u16 = 0xA;
    pub const USER_SETTINGS_SAVE: u16 = 0xB;
    pub const USER_SETTINGS_LOAD_ALL: u16 = 0xC;
    // 0xD --
    pub const DELETE_USER_SETTINGS: u16 = 0xE;
    //
    pub const FILTER_FOR_PROFANITY: u16 = 0x14;
    pub const FETCH_QOS_CONFIG: u16 = 0x15;
    pub const SET_CLIENT_METRICS: u16 = 0x16;
    pub const SET_CONNECTION_STATE: u16 = 0x17;
    pub const GET_PSS_CONFIG: u16 = 0x18;
    pub const GET_USER_OPTIONS: u16 = 0x19;
    pub const SET_USER_OPTIONS: u16 = 0x1A;
    pub const SUSPEND_USER_PING: u16 = 0x1B;
}

pub mod messaging {
    pub const COMPONENT: u16 = 0xF;

    /// Components
    pub const FETCH_MESSAGES: u16 = 0x2;
    pub const PURGE_MESSAGES: u16 = 0x3;
    pub const TOUCH_MESSAGES: u16 = 0x4;
    pub const GET_MESSAGES: u16 = 0x5;

    // Notifications
    pub const SEND_MESSAGE: u16 = 0x1;
}

pub mod association_lists {
    use tdf::ObjectType;

    pub const COMPONENT: u16 = 0x19;

    // Components
    pub const ADD_USERS_TO_LIST: u16 = 0x1;
    pub const REMOVE_USERS_FROM_LIST: u16 = 0x2;
    pub const CLEAR_LIST: u16 = 0x3;
    pub const SET_USERS_TO_LIST: u16 = 0x4;
    pub const GET_LIST_FOR_USER: u16 = 0x5;
    pub const GET_LISTS: u16 = 0x6;
    pub const SUBSCRIBE_TO_LISTS: u16 = 0x7;
    pub const UNSUBSCRIBE_TO_LISTS: u16 = 0x8;
    pub const GET_CONFIG_LISTS_INFO: u16 = 0x9;

    // Object Types
    pub const ASSOC_LIST_REF: ObjectType = ObjectType::new(COMPONENT, 1);
}

pub mod game_reporting {
    pub const COMPONENT: u16 = 0x1C;

    // Components
    pub const SUBMIT_GAME_REPORT: u16 = 0x1;
    pub const SUBMIT_OFFLINE_GAME_REPORT: u16 = 0x2;
    pub const SUBMIT_GAME_EVENTS: u16 = 0x3;
    pub const GET_GAME_REPORT_QUERY: u16 = 0x4;
    pub const GET_GAME_REPORT_QUERIES_LIST: u16 = 0x5;
    pub const GET_GAME_REPORTS: u16 = 0x6;
    pub const GET_GAME_REPORT_VIEW: u16 = 0x7;
    pub const GET_GAME_REPORT_VIEW_INFO: u16 = 0x8;
    pub const GET_GAME_REPORT_VIEW_INFO_LIST: u16 = 0x9;
    pub const GET_GAME_REPORT_TYPES: u16 = 0xA;
    pub const UPDATE_METRICS: u16 = 0xB;
    pub const GET_GAME_REPORT_COLUMN_INFO: u16 = 0xC;
    pub const GET_GAME_REPORT_COLUMN_VALUES: u16 = 0xD;
    // 0xE-0x63 --
    pub const SUBMIT_TRUSTED_MID_GAME_REPORT: u16 = 0x64;
    pub const SUBMIT_TRUSTED_END_GAME_REPORT: u16 = 0x65;

    // Notifications
    pub const GAME_REPORT_SUBMITTED: u16 = 0x72;
}

pub mod user_sessions {
    use tdf::ObjectType;

    pub const COMPONENT: u16 = 0x7802;

    // Components
    pub const UPDATE_HARDWARE_FLAGS: u16 = 0x8;
    pub const LOOKUP_USER: u16 = 0xC;
    pub const LOOKUP_USERS: u16 = 0xD;
    pub const LOOKUP_USERS_BY_PREFIX: u16 = 0xE;
    // 0xF-0x13 --
    pub const UPDATE_NETWORK_INFO: u16 = 0x14;
    // 0x15-0x16 --
    pub const LOOKUP_USER_GEO_IP_DATA: u16 = 0x17;
    pub const OVERRIDE_USER_GEO_IP_DATA: u16 = 0x18;
    pub const UPDATE_USER_SESSION_CLIENT_DATA: u16 = 0x19;
    pub const SET_USER_INFO_ATTRIBUTE: u16 = 0x1A;
    pub const RESET_USER_GEO_IP_DATA: u16 = 0x1B;
    // 0x1C-0x1F --
    pub const LOOKUP_USER_SESSION_ID: u16 = 0x20;
    pub const FETCH_LAST_LOCALE_USED_AND_AUTH_ERROR: u16 = 0x21;
    pub const FETCH_USER_FIRST_LAST_AUTH_TIME: u16 = 0x22;
    pub const RESUME_SESSION: u16 = 0x23;

    // Notifications
    pub const USER_SESSION_EXTENDED_DATA_UPDATE: u16 = 0x1;
    pub const USER_ADDED: u16 = 0x2;
    pub const USER_REMOVED: u16 = 0x3;
    pub const USER_SESSION_DISCONNECTED: u16 = 0x4;
    pub const USER_UPDATED: u16 = 0x5;
    pub const USER_AUTHENTICATED: u16 = 0x8;
    pub const USER_UNAUTHENTICATED: u16 = 0x9;

    // Object Types
    pub const PLAYER_TYPE: ObjectType = ObjectType::new(COMPONENT, 1);
}
