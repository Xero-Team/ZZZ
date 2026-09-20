#![allow(non_snake_case)]

pub mod error;
mod macros;
mod typed_envelope;

pub use error::*;
pub use prost::{DecodeError, Message};
use std::{
    cmp,
    fmt::Debug,
    iter, mem,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use typed_envelope::*;

include!(concat!(env!("OUT_DIR"), "/zzz.messages.rs"));

pub const REMOTE_SERVER_PEER_ID: PeerId = PeerId { owner_id: 0, id: 0 };
pub const REMOTE_SERVER_PROJECT_ID: u64 = 0;

messages!(
    (Ack, Foreground),
    (ActivateToolchain, Foreground),
    (ActiveToolchain, Foreground),
    (ActiveToolchainResponse, Foreground),
    (ResolveToolchain, Background),
    (ResolveToolchainResponse, Background),
    (AddNotification, Foreground),
    (AddWorktree, Foreground),
    (AddWorktreeResponse, Foreground),
    (AllocateWorktreeId, Foreground),
    (AllocateWorktreeIdResponse, Foreground),
    (ApplyCodeAction, Background),
    (ApplyCodeActionResponse, Background),
    (ApplyCompletionAdditionalEdits, Background),
    (ApplyCompletionAdditionalEditsResponse, Background),
    (BlameBuffer, Foreground),
    (BlameBufferResponse, Foreground),
    (BufferReloaded, Foreground),
    (BufferSaved, Foreground),
    (CancelLanguageServerWork, Foreground),
    (CloseBuffer, Foreground),
    (Commit, Background),
    (RunGitHook, Background),
    (CopyProjectEntry, Foreground),
    (CreateBufferForPeer, Foreground),
    (CreateImageForPeer, Foreground),
    (CreateFileForPeer, Foreground),
    (CreateProjectEntry, Foreground),
    (DeleteNotification, Foreground),
    (DeleteProjectEntry, Foreground),
    (DownloadFileByPath, Background),
    (DownloadFileResponse, Background),
    (EndStream, Foreground),
    (Error, Foreground),
    (ExpandProjectEntry, Foreground),
    (ExpandProjectEntryResponse, Foreground),
    (FindSearchCandidates, Background),
    (FlushBufferedMessages, Foreground),
    (ExpandAllForProjectEntry, Foreground),
    (ExpandAllForProjectEntryResponse, Foreground),
    (ApplyCodeActionKind, Foreground),
    (ApplyCodeActionKindResponse, Foreground),
    (FormatBuffers, Foreground),
    (FormatBuffersResponse, Foreground),
    (GetCodeActions, Background),
    (GetCodeActionsResponse, Background),
    (GetCompletions, Background),
    (GetCompletionsResponse, Background),
    (GetDeclaration, Background),
    (GetDeclarationResponse, Background),
    (GetDefinition, Background),
    (GetDefinitionResponse, Background),
    (GetDocumentHighlights, Background),
    (GetDocumentHighlightsResponse, Background),
    (GetDocumentSymbols, Background),
    (GetDocumentSymbolsResponse, Background),
    (GetHover, Background),
    (GetHoverResponse, Background),
    (GetNotifications, Foreground),
    (GetNotificationsResponse, Foreground),
    (GetPathMetadata, Background),
    (GetPathMetadataResponse, Background),
    (GetPermalinkToLine, Foreground),
    (GetProcesses, Background),
    (GetProcessesResponse, Background),
    (GetPermalinkToLineResponse, Foreground),
    (GetProjectSymbols, Background),
    (GetProjectSymbolsResponse, Background),
    (GetReferences, Background),
    (GetReferencesResponse, Background),
    (GetSignatureHelp, Background),
    (GetSignatureHelpResponse, Background),
    (GetTypeDefinition, Background),
    (GetTypeDefinitionResponse, Background),
    (GetImplementation, Background),
    (GetImplementationResponse, Background),
    (OpenUnstagedDiff, Foreground),
    (OpenUnstagedDiffResponse, Foreground),
    (OpenUncommittedDiff, Foreground),
    (OpenUncommittedDiffResponse, Foreground),
    (GetUsers, Foreground),
    (GitGetBranches, Background),
    (GitBranchesResponse, Background),
    (Hello, Foreground),
    (HideToast, Background),
    (InlayHints, Background),
    (InlayHintsResponse, Background),
    (SemanticTokens, Background),
    (SemanticTokensResponse, Background),
    (InstallExtension, Background),
    (LanguageServerLog, Foreground),
    (LanguageServerPromptRequest, Foreground),
    (LanguageServerPromptResponse, Foreground),
    (LanguageServerShowDocumentRequest, Background),
    (LinkedEditingRange, Background),
    (LinkedEditingRangeResponse, Background),
    (ListRemoteDirectory, Background),
    (ListRemoteDirectoryResponse, Background),
    (ListToolchains, Foreground),
    (ListToolchainsResponse, Foreground),
    (LoadCommitDiff, Foreground),
    (LoadCommitDiffResponse, Foreground),
    (LspExtExpandMacro, Background),
    (LspExtExpandMacroResponse, Background),
    (LspExtOpenDocs, Background),
    (LspExtOpenDocsResponse, Background),
    (LspExtRunnables, Background),
    (LspExtRunnablesResponse, Background),
    (LspExtSwitchSourceHeader, Background),
    (LspExtSwitchSourceHeaderResponse, Background),
    (LspExtGoToParentModule, Background),
    (LspExtGoToParentModuleResponse, Background),
    (ExecuteLspCommand, Background),
    (ExecuteLspCommandResponse, Background),
    (LspExtCancelFlycheck, Background),
    (LspExtRunFlycheck, Background),
    (LspExtClearFlycheck, Background),
    (MarkNotificationRead, Foreground),
    (LspQuery, Background),
    (LspQueryResponse, Background),
    (OnTypeFormatting, Background),
    (OnTypeFormattingResponse, Background),
    (OpenBufferById, Background),
    (OpenBufferByPath, Background),
    (OpenImageByPath, Background),
    (OpenBufferForSymbol, Background),
    (OpenBufferForSymbolResponse, Background),
    (OpenBufferResponse, Background),
    (OpenImageResponse, Background),
    (OpenCommitMessageBuffer, Background),
    (OpenNewBuffer, Foreground),
    (OpenServerSettings, Foreground),
    (PerformRename, Background),
    (PerformRenameResponse, Background),
    (Ping, Foreground),
    (PrepareRename, Background),
    (PrepareRenameResponse, Background),
    (ProjectEntryResponse, Foreground),
    (RefreshInlayHints, Background),
    (RefreshSemanticTokens, Background),
    (RegisterBufferWithLanguageServers, Background),
    (ReloadBuffers, Foreground),
    (ReloadBuffersResponse, Foreground),
    (RemoveWorktree, Foreground),
    (RenameProjectEntry, Foreground),
    (ResolveCompletionDocumentation, Background),
    (ResolveCompletionDocumentationResponse, Background),
    (ResolveInlayHint, Background),
    (ResolveInlayHintResponse, Background),
    (GetDocumentColor, Background),
    (GetDocumentColorResponse, Background),
    (GetColorPresentation, Background),
    (GetColorPresentationResponse, Background),
    (GetDocumentLinks, Background),
    (GetDocumentLinksResponse, Background),
    (ResolveDocumentLink, Background),
    (ResolveDocumentLinkResponse, Background),
    (GetFoldingRanges, Background),
    (GetFoldingRangesResponse, Background),
    (RefreshCodeLens, Background),
    (GetCodeLens, Background),
    (GetCodeLensResponse, Background),
    (RestartLanguageServers, Foreground),
    (StopLanguageServers, Background),
    (SaveBuffer, Foreground),
    (ShutdownRemoteServer, Foreground),
    (Stage, Background),
    (StartLanguageServer, Foreground),
    (SyncExtensions, Background),
    (SyncExtensionsResponse, Background),
    (BreakpointsForFile, Background),
    (ToggleBreakpoint, Foreground),
    (SynchronizeBuffers, Foreground),
    (SynchronizeBuffersResponse, Foreground),
    (TaskContext, Background),
    (TaskContextForLocation, Background),
    (Test, Foreground),
    (Toast, Background),
    (Unstage, Background),
    (Stash, Background),
    (StashPop, Background),
    (StashApply, Background),
    (StashDrop, Background),
    (UpdateBuffer, Foreground),
    (UpdateBufferFile, Foreground),
    (UpdateDiagnosticSummary, Foreground),
    (UpdateDiffBases, Foreground),
    (UpdateGitBranch, Background),
    (UpdateLanguageServer, Foreground),
    (UpdateNotification, Foreground),
    (UpdateProject, Foreground),
    (UpdateWorktree, Foreground),
    (UpdateWorktreeSettings, Foreground),
    (UpdateUserSettings, Background),
    (UpdateRepository, Foreground),
    (RemoveRepository, Foreground),
    (UsersResponse, Foreground),
    (GitReset, Background),
    (GitDeleteBranch, Background),
    (GitCheckoutFiles, Background),
    (GitShow, Background),
    (GitCommitDetails, Background),
    (GitCreateCheckpoint, Background),
    (GitCreateCheckpointResponse, Background),
    (GitCreateArchiveCheckpoint, Background),
    (GitCreateArchiveCheckpointResponse, Background),
    (GitRestoreCheckpoint, Background),
    (GitRestoreArchiveCheckpoint, Background),
    (GitCompareCheckpoints, Background),
    (GitCompareCheckpointsResponse, Background),
    (GitDiffCheckpoints, Background),
    (GitDiffCheckpointsResponse, Background),
    (SetIndexText, Background),
    (Push, Background),
    (Fetch, Background),
    (GetRemotes, Background),
    (GetRemotesResponse, Background),
    (Pull, Background),
    (RemoteMessageResponse, Background),
    (AskPassRequest, Background),
    (AskPassResponse, Background),
    (GitCreateRemote, Background),
    (GitRemoveRemote, Background),
    (GitCreateBranch, Background),
    (GitChangeBranch, Background),
    (GitRenameBranch, Background),
    (TrustWorktrees, Background),
    (RestrictWorktrees, Background),
    (CheckForPushedCommits, Background),
    (CheckForPushedCommitsResponse, Background),
    (GitDiff, Background),
    (GitDiffResponse, Background),
    (GitInit, Background),
    (GetDebugAdapterBinary, Background),
    (DebugAdapterBinary, Background),
    (RunDebugLocators, Background),
    (DebugRequest, Background),
    (LogToDebugConsole, Background),
    (GetDocumentDiagnostics, Background),
    (GetDocumentDiagnosticsResponse, Background),
    (PullWorkspaceDiagnostics, Background),
    (GetDefaultBranch, Background),
    (GetDefaultBranchResponse, Background),
    (GetTreeDiff, Background),
    (GetTreeDiffResponse, Background),
    (GetBlobContent, Background),
    (GetBlobContentResponse, Background),
    (GitClone, Background),
    (GitCloneResponse, Background),
    (ToggleLspLogs, Background),
    (GetDirectoryEnvironment, Background),
    (DirectoryEnvironment, Background),
    (GetAgentServerCommand, Background),
    (AgentServerCommand, Background),
    (GetContextServerCommand, Background),
    (ContextServerCommand, Background),
    (ExternalAgentsUpdated, Background),
    (ExternalAgentLoadingStatusUpdated, Background),
    (NewExternalAgentVersionAvailable, Background),
    (RemoteStarted, Background),
    (GitGetWorktrees, Background),
    (GitGetHeadSha, Background),
    (GitGetHeadShaResponse, Background),
    (GitEditRef, Background),
    (GitRepairWorktrees, Background),
    (GetCommitData, Background),
    (GetCommitDataResponse, Background),
    (GetInitialGraphData, Background),
    (GetInitialGraphDataResponse, Background),
    (SearchCommits, Background),
    (SearchCommitsResponse, Background),
    (GitWorktreesResponse, Background),
    (GitCreateWorktree, Background),
    (GitRemoveWorktree, Background),
    (GitRenameWorktree, Background),
    (GitWorktreeCreatedAt, Background),
    (GitWorktreeCreatedAtResponse, Background),
    (FindSearchCandidatesChunk, Background),
    (FindSearchCandidatesCancelled, Background),
    (SpawnKernel, Background),
    (SpawnKernelResponse, Background),
    (KillKernel, Background),
    (GetRemoteProfilingData, Background),
    (GetRemoteProfilingDataResponse, Background),
);

request_messages!(
    (AllocateWorktreeId, AllocateWorktreeIdResponse),
    (ApplyCodeAction, ApplyCodeActionResponse),
    (
        ApplyCompletionAdditionalEdits,
        ApplyCompletionAdditionalEditsResponse
    ),
    (Commit, Ack),
    (RunGitHook, Ack),
    (CopyProjectEntry, ProjectEntryResponse),
    (CreateProjectEntry, ProjectEntryResponse),
    (DeleteProjectEntry, ProjectEntryResponse),
    (DownloadFileByPath, DownloadFileResponse),
    (ExpandProjectEntry, ExpandProjectEntryResponse),
    (ExpandAllForProjectEntry, ExpandAllForProjectEntryResponse),
    (ApplyCodeActionKind, ApplyCodeActionKindResponse),
    (FormatBuffers, FormatBuffersResponse),
    (GetCodeActions, GetCodeActionsResponse),
    (GetCompletions, GetCompletionsResponse),
    (GetDefinition, GetDefinitionResponse),
    (GetDeclaration, GetDeclarationResponse),
    (GetImplementation, GetImplementationResponse),
    (GetDocumentHighlights, GetDocumentHighlightsResponse),
    (GetDocumentSymbols, GetDocumentSymbolsResponse),
    (GetHover, GetHoverResponse),
    (GetNotifications, GetNotificationsResponse),
    (GetProjectSymbols, GetProjectSymbolsResponse),
    (GetReferences, GetReferencesResponse),
    (GetSignatureHelp, GetSignatureHelpResponse),
    (OpenUnstagedDiff, OpenUnstagedDiffResponse),
    (OpenUncommittedDiff, OpenUncommittedDiffResponse),
    (GetTypeDefinition, GetTypeDefinitionResponse),
    (LinkedEditingRange, LinkedEditingRangeResponse),
    (ListRemoteDirectory, ListRemoteDirectoryResponse),
    (GetUsers, UsersResponse),
    (InlayHints, InlayHintsResponse),
    (SemanticTokens, SemanticTokensResponse),
    (GetCodeLens, GetCodeLensResponse),
    (LoadCommitDiff, LoadCommitDiffResponse),
    (MarkNotificationRead, Ack),
    (OnTypeFormatting, OnTypeFormattingResponse),
    (OpenBufferById, OpenBufferResponse),
    (OpenBufferByPath, OpenBufferResponse),
    (OpenImageByPath, OpenImageResponse),
    (OpenBufferForSymbol, OpenBufferForSymbolResponse),
    (OpenCommitMessageBuffer, OpenBufferResponse),
    (OpenNewBuffer, OpenBufferResponse),
    (PerformRename, PerformRenameResponse),
    (Ping, Ack),
    (PrepareRename, PrepareRenameResponse),
    (RefreshInlayHints, Ack),
    (RefreshSemanticTokens, Ack),
    (RefreshCodeLens, Ack),
    (ReloadBuffers, ReloadBuffersResponse),
    (RenameProjectEntry, ProjectEntryResponse),
    (
        ResolveCompletionDocumentation,
        ResolveCompletionDocumentationResponse
    ),
    (ResolveInlayHint, ResolveInlayHintResponse),
    (GetDocumentColor, GetDocumentColorResponse),
    (GetDocumentLinks, GetDocumentLinksResponse),
    (ResolveDocumentLink, ResolveDocumentLinkResponse),
    (GetFoldingRanges, GetFoldingRangesResponse),
    (GetColorPresentation, GetColorPresentationResponse),
    (SaveBuffer, BufferSaved),
    (Stage, Ack),
    (FindSearchCandidates, Ack),
    (SynchronizeBuffers, SynchronizeBuffersResponse),
    (TaskContextForLocation, TaskContext),
    (Test, Test),
    (Unstage, Ack),
    (Stash, Ack),
    (StashPop, Ack),
    (StashApply, Ack),
    (StashDrop, Ack),
    (UpdateBuffer, Ack),
    (UpdateProject, Ack),
    (UpdateWorktree, Ack),
    (UpdateRepository, Ack),
    (RemoveRepository, Ack),
    (LspExtExpandMacro, LspExtExpandMacroResponse),
    (LspExtOpenDocs, LspExtOpenDocsResponse),
    (LspExtRunnables, LspExtRunnablesResponse),
    (BlameBuffer, BlameBufferResponse),
    (LspQuery, Ack),
    (LspQueryResponse, Ack),
    (RestartLanguageServers, Ack),
    (StopLanguageServers, Ack),
    (LspExtSwitchSourceHeader, LspExtSwitchSourceHeaderResponse),
    (LspExtGoToParentModule, LspExtGoToParentModuleResponse),
    (ExecuteLspCommand, ExecuteLspCommandResponse),
    (LspExtCancelFlycheck, Ack),
    (LspExtRunFlycheck, Ack),
    (LspExtClearFlycheck, Ack),
    (AddWorktree, AddWorktreeResponse),
    (ShutdownRemoteServer, Ack),
    (RemoveWorktree, Ack),
    (OpenServerSettings, OpenBufferResponse),
    (GetPermalinkToLine, GetPermalinkToLineResponse),
    (FlushBufferedMessages, Ack),
    (LanguageServerPromptRequest, LanguageServerPromptResponse),
    (LanguageServerShowDocumentRequest, Ack),
    (GitGetBranches, GitBranchesResponse),
    (UpdateGitBranch, Ack),
    (ListToolchains, ListToolchainsResponse),
    (ActivateToolchain, Ack),
    (ActiveToolchain, ActiveToolchainResponse),
    (ResolveToolchain, ResolveToolchainResponse),
    (GetPathMetadata, GetPathMetadataResponse),
    (CancelLanguageServerWork, Ack),
    (SyncExtensions, SyncExtensionsResponse),
    (InstallExtension, Ack),
    (RegisterBufferWithLanguageServers, Ack),
    (GitShow, GitCommitDetails),
    (GitCreateCheckpoint, GitCreateCheckpointResponse),
    (
        GitCreateArchiveCheckpoint,
        GitCreateArchiveCheckpointResponse
    ),
    (GitRestoreCheckpoint, Ack),
    (GitRestoreArchiveCheckpoint, Ack),
    (GitCompareCheckpoints, GitCompareCheckpointsResponse),
    (GitDiffCheckpoints, GitDiffCheckpointsResponse),
    (GitReset, Ack),
    (GitDeleteBranch, Ack),
    (GitCheckoutFiles, Ack),
    (SetIndexText, Ack),
    (Push, RemoteMessageResponse),
    (Fetch, RemoteMessageResponse),
    (GetRemotes, GetRemotesResponse),
    (Pull, RemoteMessageResponse),
    (AskPassRequest, AskPassResponse),
    (GitCreateRemote, Ack),
    (GitRemoveRemote, Ack),
    (GitCreateBranch, Ack),
    (GitChangeBranch, Ack),
    (GitRenameBranch, Ack),
    (CheckForPushedCommits, CheckForPushedCommitsResponse),
    (GitDiff, GitDiffResponse),
    (GitInit, Ack),
    (ToggleBreakpoint, Ack),
    (GetDebugAdapterBinary, DebugAdapterBinary),
    (RunDebugLocators, DebugRequest),
    (GetDocumentDiagnostics, GetDocumentDiagnosticsResponse),
    (PullWorkspaceDiagnostics, Ack),
    (GetDefaultBranch, GetDefaultBranchResponse),
    (GetBlobContent, GetBlobContentResponse),
    (GetTreeDiff, GetTreeDiffResponse),
    (GitClone, GitCloneResponse),
    (ToggleLspLogs, Ack),
    (GetDirectoryEnvironment, DirectoryEnvironment),
    (GetProcesses, GetProcessesResponse),
    (GetAgentServerCommand, AgentServerCommand),
    (GetContextServerCommand, ContextServerCommand),
    (RemoteStarted, Ack),
    (GitGetWorktrees, GitWorktreesResponse),
    (GitGetHeadSha, GitGetHeadShaResponse),
    (GitEditRef, Ack),
    (GitRepairWorktrees, Ack),
    (GetCommitData, GetCommitDataResponse),
    (GetInitialGraphData, GetInitialGraphDataResponse),
    (SearchCommits, SearchCommitsResponse),
    (GitCreateWorktree, Ack),
    (GitRemoveWorktree, Ack),
    (GitRenameWorktree, Ack),
    (GitWorktreeCreatedAt, GitWorktreeCreatedAtResponse),
    (TrustWorktrees, Ack),
    (RestrictWorktrees, Ack),
    (FindSearchCandidatesChunk, Ack),
    (SpawnKernel, SpawnKernelResponse),
    (KillKernel, Ack),
    (GetRemoteProfilingData, GetRemoteProfilingDataResponse),
);

lsp_messages!(
    (GetReferences, GetReferencesResponse, true),
    (GetDocumentColor, GetDocumentColorResponse, true),
    (GetFoldingRanges, GetFoldingRangesResponse, true),
    (GetDocumentSymbols, GetDocumentSymbolsResponse, true),
    (GetDocumentLinks, GetDocumentLinksResponse, true),
    (GetHover, GetHoverResponse, true),
    (GetCodeActions, GetCodeActionsResponse, true),
    (GetSignatureHelp, GetSignatureHelpResponse, true),
    (GetCodeLens, GetCodeLensResponse, true),
    (GetDocumentDiagnostics, GetDocumentDiagnosticsResponse, true),
    (GetDefinition, GetDefinitionResponse, true),
    (GetDeclaration, GetDeclarationResponse, true),
    (GetTypeDefinition, GetTypeDefinitionResponse, true),
    (GetImplementation, GetImplementationResponse, true),
    (InlayHints, InlayHintsResponse, false),
    (SemanticTokens, SemanticTokensResponse, true)
);

entity_messages!(
    {project_id, UpdateProject},
    AddWorktree,
    AllocateWorktreeId,
    ApplyCodeAction,
    ApplyCompletionAdditionalEdits,
    BlameBuffer,
    BufferReloaded,
    BufferSaved,
    CloseBuffer,
    Commit,
    RunGitHook,
    GetColorPresentation,
    CopyProjectEntry,
    CreateBufferForPeer,
    CreateFileForPeer,
    CreateImageForPeer,
    CreateProjectEntry,
    GetDocumentColor,
    GetDocumentLinks,
    ResolveDocumentLink,
    GetFoldingRanges,
    DeleteProjectEntry,
    ExpandProjectEntry,
    ExpandAllForProjectEntry,
    FindSearchCandidates,
    ApplyCodeActionKind,
    FormatBuffers,
    GetCodeActions,
    GetCodeLens,
    GetCompletions,
    GetDefinition,
    GetDeclaration,
    GetImplementation,
    GetDocumentHighlights,
    GetDocumentSymbols,
    GetHover,
    GetProjectSymbols,
    GetReferences,
    GetSignatureHelp,
    OpenUnstagedDiff,
    OpenUncommittedDiff,
    GetTypeDefinition,
    InlayHints,
    SemanticTokens,
    SpawnKernel,
    KillKernel,
    LinkedEditingRange,
    LoadCommitDiff,
    LspQuery,
    LspQueryResponse,
    RestartLanguageServers,
    StopLanguageServers,
    OnTypeFormatting,
    OpenNewBuffer,
    OpenBufferById,
    OpenBufferByPath,
    OpenImageByPath,
    OpenBufferForSymbol,
    OpenCommitMessageBuffer,
    PerformRename,
    PrepareRename,
    RefreshInlayHints,
    RefreshSemanticTokens,
    RefreshCodeLens,
    ReloadBuffers,
    RenameProjectEntry,
    ResolveCompletionDocumentation,
    ResolveInlayHint,
    SaveBuffer,
    Stage,
    StartLanguageServer,
    SynchronizeBuffers,
    TaskContextForLocation,
    Unstage,
    Stash,
    StashPop,
    StashApply,
    StashDrop,
    UpdateBuffer,
    UpdateBufferFile,
    UpdateDiagnosticSummary,
    UpdateDiffBases,
    UpdateLanguageServer,
    UpdateProject,
    UpdateWorktree,
    UpdateRepository,
    RemoveRepository,
    UpdateWorktreeSettings,
    UpdateUserSettings,
    LspExtExpandMacro,
    LspExtOpenDocs,
    LspExtRunnables,
    LspExtSwitchSourceHeader,
    LspExtGoToParentModule,
    ExecuteLspCommand,
    LspExtCancelFlycheck,
    LspExtRunFlycheck,
    LspExtClearFlycheck,
    LanguageServerLog,
    Toast,
    HideToast,
    OpenServerSettings,
    GetPermalinkToLine,
    LanguageServerPromptRequest,
    LanguageServerShowDocumentRequest,
    GitGetBranches,
    UpdateGitBranch,
    ListToolchains,
    ActivateToolchain,
    ActiveToolchain,
    ResolveToolchain,
    GetPathMetadata,
    GetProcesses,
    CancelLanguageServerWork,
    RegisterBufferWithLanguageServers,
    GitShow,
    GitCreateCheckpoint,
    GitRestoreCheckpoint,
    GitCompareCheckpoints,
    GitDiffCheckpoints,
    GitReset,
    GitDeleteBranch,
    GitCheckoutFiles,
    SetIndexText,
    ToggleLspLogs,
    GetDirectoryEnvironment,

    Push,
    Fetch,
    GetRemotes,
    Pull,
    AskPassRequest,
    GitChangeBranch,
    GitRenameBranch,
    GitCreateBranch,
    GitCreateRemote,
    GitRemoveRemote,
    CheckForPushedCommits,
    GitDiff,
    GitInit,
    BreakpointsForFile,
    ToggleBreakpoint,
    RunDebugLocators,
    GetDebugAdapterBinary,
    LogToDebugConsole,
    GetDocumentDiagnostics,
    PullWorkspaceDiagnostics,
    GetDefaultBranch,
    GetTreeDiff,
    GetBlobContent,
    GitClone,
    GetAgentServerCommand,
    GetContextServerCommand,
    ExternalAgentsUpdated,
    ExternalAgentLoadingStatusUpdated,
    NewExternalAgentVersionAvailable,
    GitGetWorktrees,
    GitGetHeadSha,
    GitEditRef,
    GitRepairWorktrees,
    GetCommitData,
    GetInitialGraphData,
    SearchCommits,
    GitCreateArchiveCheckpoint,
    GitRestoreArchiveCheckpoint,
    GitCreateWorktree,
    GitRemoveWorktree,
    GitRenameWorktree,
    GitWorktreeCreatedAt,
    TrustWorktrees,
    RestrictWorktrees,
    FindSearchCandidatesChunk,
    FindSearchCandidatesCancelled,
    DownloadFileByPath,
    GetRemoteProfilingData
);

impl From<Timestamp> for SystemTime {
    fn from(val: Timestamp) -> Self {
        UNIX_EPOCH
            .checked_add(Duration::new(val.seconds, val.nanos))
            .unwrap()
    }
}

impl From<SystemTime> for Timestamp {
    fn from(time: SystemTime) -> Self {
        let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
        Self {
            seconds: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        }
    }
}

impl From<u128> for Nonce {
    fn from(nonce: u128) -> Self {
        let upper_half = (nonce >> 64) as u64;
        let lower_half = nonce as u64;
        Self {
            upper_half,
            lower_half,
        }
    }
}

impl From<Nonce> for u128 {
    fn from(nonce: Nonce) -> Self {
        let upper_half = (nonce.upper_half as u128) << 64;
        let lower_half = nonce.lower_half as u128;
        upper_half | lower_half
    }
}

#[cfg(any(test, feature = "test-support"))]
pub const MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE: usize = 2;
#[cfg(not(any(test, feature = "test-support")))]
pub const MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE: usize = 256;

pub fn split_worktree_update(mut message: UpdateWorktree) -> impl Iterator<Item = UpdateWorktree> {
    let mut done = false;

    iter::from_fn(move || {
        if done {
            return None;
        }

        let updated_entries_chunk_size = cmp::min(
            message.updated_entries.len(),
            MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE,
        );
        let updated_entries: Vec<_> = message
            .updated_entries
            .drain(..updated_entries_chunk_size)
            .collect();

        let removed_entries_chunk_size = cmp::min(
            message.removed_entries.len(),
            MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE,
        );
        let removed_entries = message
            .removed_entries
            .drain(..removed_entries_chunk_size)
            .collect();

        let mut updated_repositories = Vec::new();
        let mut limit = MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE;
        while let Some(repo) = message.updated_repositories.first_mut() {
            let updated_statuses_limit = cmp::min(repo.updated_statuses.len(), limit);
            let removed_statuses_limit = cmp::min(repo.removed_statuses.len(), limit);

            updated_repositories.push(RepositoryEntry {
                repository_id: repo.repository_id,
                branch_summary: repo.branch_summary.clone(),
                updated_statuses: repo
                    .updated_statuses
                    .drain(..updated_statuses_limit)
                    .collect(),
                removed_statuses: repo
                    .removed_statuses
                    .drain(..removed_statuses_limit)
                    .collect(),
                current_merge_conflicts: repo.current_merge_conflicts.clone(),
            });
            if repo.removed_statuses.is_empty() && repo.updated_statuses.is_empty() {
                message.updated_repositories.remove(0);
            }
            limit = limit.saturating_sub(removed_statuses_limit + updated_statuses_limit);
            if limit == 0 {
                break;
            }
        }

        done = message.updated_entries.is_empty()
            && message.removed_entries.is_empty()
            && message.updated_repositories.is_empty();

        let removed_repositories = if done {
            mem::take(&mut message.removed_repositories)
        } else {
            Default::default()
        };

        Some(UpdateWorktree {
            project_id: message.project_id,
            worktree_id: message.worktree_id,
            root_name: message.root_name.clone(),
            abs_path: message.abs_path.clone(),
            root_repo_common_dir: message.root_repo_common_dir.clone(),
            root_repo_is_linked_worktree: message.root_repo_is_linked_worktree,
            updated_entries,
            removed_entries,
            scan_id: message.scan_id,
            is_last_update: done && message.is_last_update,
            updated_repositories,
            removed_repositories,
        })
    })
}

pub fn split_repository_update(
    mut update: UpdateRepository,
) -> impl Iterator<Item = UpdateRepository> {
    let mut updated_statuses_iter = mem::take(&mut update.updated_statuses).into_iter().fuse();
    let mut removed_statuses_iter = mem::take(&mut update.removed_statuses).into_iter().fuse();
    let branch_list = mem::take(&mut update.branch_list);
    let branch_list_error = update.branch_list_error.take();
    std::iter::from_fn({
        let update = update.clone();
        move || {
            let updated_statuses = updated_statuses_iter
                .by_ref()
                .take(MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE)
                .collect::<Vec<_>>();
            let removed_statuses = removed_statuses_iter
                .by_ref()
                .take(MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE)
                .collect::<Vec<_>>();
            if updated_statuses.is_empty() && removed_statuses.is_empty() {
                return None;
            }
            Some(UpdateRepository {
                updated_statuses,
                removed_statuses,
                branch_list: Vec::new(),
                branch_list_error: None,
                is_last_update: false,
                ..update.clone()
            })
        }
    })
    .chain([UpdateRepository {
        updated_statuses: Vec::new(),
        removed_statuses: Vec::new(),
        branch_list,
        branch_list_error,
        is_last_update: true,
        ..update
    }])
}

impl LspQuery {
    pub fn query_name_and_write_permissions(&self) -> (&str, bool) {
        match self.request {
            Some(lsp_query::Request::GetHover(_)) => ("GetHover", false),
            Some(lsp_query::Request::GetCodeActions(_)) => ("GetCodeActions", true),
            Some(lsp_query::Request::GetSignatureHelp(_)) => ("GetSignatureHelp", false),
            Some(lsp_query::Request::GetCodeLens(_)) => ("GetCodeLens", true),
            Some(lsp_query::Request::GetDocumentDiagnostics(_)) => {
                ("GetDocumentDiagnostics", false)
            }
            Some(lsp_query::Request::GetDefinition(_)) => ("GetDefinition", false),
            Some(lsp_query::Request::GetDeclaration(_)) => ("GetDeclaration", false),
            Some(lsp_query::Request::GetTypeDefinition(_)) => ("GetTypeDefinition", false),
            Some(lsp_query::Request::GetImplementation(_)) => ("GetImplementation", false),
            Some(lsp_query::Request::GetReferences(_)) => ("GetReferences", false),
            Some(lsp_query::Request::GetDocumentColor(_)) => ("GetDocumentColor", false),
            Some(lsp_query::Request::GetFoldingRanges(_)) => ("GetFoldingRanges", false),
            Some(lsp_query::Request::GetDocumentSymbols(_)) => ("GetDocumentSymbols", false),
            Some(lsp_query::Request::GetDocumentLinks(_)) => ("GetDocumentLinks", false),
            Some(lsp_query::Request::InlayHints(_)) => ("InlayHints", false),
            Some(lsp_query::Request::SemanticTokens(_)) => ("SemanticTokens", false),
            None => ("<unknown>", true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_converting_peer_id_from_and_to_u64() {
        let peer_id = PeerId {
            owner_id: 10,
            id: 3,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
        let peer_id = PeerId {
            owner_id: u32::MAX,
            id: 3,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
        let peer_id = PeerId {
            owner_id: 10,
            id: u32::MAX,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
        let peer_id = PeerId {
            owner_id: u32::MAX,
            id: u32::MAX,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
    }

    #[test]
    fn test_split_repository_update_keeps_branch_list_on_final_chunk() {
        let update = UpdateRepository {
            updated_statuses: vec![
                StatusEntry::default(),
                StatusEntry::default(),
                StatusEntry::default(),
            ],
            branch_list: vec![Branch {
                ref_name: "refs/heads/main".into(),
                ..Default::default()
            }],
            branch_list_error: Some("partial branch scan".into()),
            ..Default::default()
        };

        let chunks = split_repository_update(update).collect::<Vec<_>>();

        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].branch_list.is_empty());
        assert!(chunks[1].branch_list.is_empty());
        assert_eq!(chunks[2].branch_list.len(), 1);
        assert_eq!(chunks[2].branch_list[0].ref_name, "refs/heads/main");
        assert_eq!(chunks[0].branch_list_error, None);
        assert_eq!(chunks[1].branch_list_error, None);
        assert_eq!(
            chunks[2].branch_list_error.as_deref(),
            Some("partial branch scan")
        );
        assert!(!chunks[0].is_last_update);
        assert!(!chunks[1].is_last_update);
        assert!(chunks[2].is_last_update);
    }

    #[test]
    fn test_timestamp_roundtrip_and_pre_epoch_conversion() {
        let timestamp = Timestamp {
            seconds: 123,
            nanos: 400,
        };
        let time = SystemTime::from(timestamp);

        let timestamp = Timestamp::from(time);
        assert_eq!(timestamp.seconds, 123);
        assert_eq!(timestamp.nanos, 400);
        assert_eq!(SystemTime::from(timestamp), time);

        let pre_epoch_time = UNIX_EPOCH.checked_sub(Duration::from_secs(1)).unwrap();
        let timestamp = Timestamp::from(pre_epoch_time);
        assert_eq!(timestamp.seconds, 0);
        assert_eq!(timestamp.nanos, 0);
    }

    #[test]
    fn test_nonce_roundtrip() {
        let nonce_value = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210_u128;

        let nonce = Nonce::from(nonce_value);
        assert_eq!(nonce.upper_half, 0x0123_4567_89ab_cdef);
        assert_eq!(nonce.lower_half, 0xfedc_ba98_7654_3210);
        assert_eq!(u128::from(nonce), nonce_value);
    }

    #[test]
    fn test_split_worktree_update_chunks_and_preserves_final_metadata() {
        let update = UpdateWorktree {
            project_id: 7,
            worktree_id: 11,
            root_name: "workspace".into(),
            abs_path: "/tmp/workspace".into(),
            root_repo_common_dir: Some("/tmp/workspace/.git".into()),
            root_repo_is_linked_worktree: true,
            updated_entries: vec![Entry::default(), Entry::default(), Entry::default()],
            removed_entries: vec![1, 2, 3],
            scan_id: 99,
            is_last_update: true,
            updated_repositories: vec![RepositoryEntry {
                repository_id: 5,
                branch_summary: Some(Branch {
                    ref_name: "refs/heads/main".into(),
                    ..Default::default()
                }),
                updated_statuses: vec![
                    StatusEntry::default(),
                    StatusEntry::default(),
                    StatusEntry::default(),
                ],
                removed_statuses: vec!["deleted.rs".into()],
                current_merge_conflicts: vec!["conflict.rs".into()],
            }],
            removed_repositories: vec![21, 34],
        };

        let chunks = split_worktree_update(update).collect::<Vec<_>>();

        assert_eq!(chunks.len(), 2);

        assert_eq!(chunks[0].project_id, 7);
        assert_eq!(chunks[0].worktree_id, 11);
        assert_eq!(chunks[0].root_name, "workspace");
        assert_eq!(chunks[0].abs_path, "/tmp/workspace");
        assert_eq!(
            chunks[0].root_repo_common_dir.as_deref(),
            Some("/tmp/workspace/.git")
        );
        assert!(chunks[0].root_repo_is_linked_worktree);
        assert_eq!(chunks[0].updated_entries.len(), 2);
        assert_eq!(chunks[0].removed_entries, vec![1, 2]);
        assert_eq!(chunks[0].updated_repositories.len(), 1);
        assert_eq!(chunks[0].updated_repositories[0].repository_id, 5);
        assert_eq!(
            chunks[0].updated_repositories[0]
                .branch_summary
                .as_ref()
                .map(|branch| branch.ref_name.as_str()),
            Some("refs/heads/main")
        );
        assert_eq!(chunks[0].updated_repositories[0].updated_statuses.len(), 2);
        assert_eq!(
            chunks[0].updated_repositories[0].removed_statuses,
            vec!["deleted.rs"]
        );
        assert!(chunks[0].removed_repositories.is_empty());
        assert!(!chunks[0].is_last_update);

        assert_eq!(chunks[1].updated_entries.len(), 1);
        assert_eq!(chunks[1].removed_entries, vec![3]);
        assert_eq!(chunks[1].updated_repositories.len(), 1);
        assert_eq!(chunks[1].updated_repositories[0].updated_statuses.len(), 1);
        assert!(
            chunks[1].updated_repositories[0]
                .removed_statuses
                .is_empty()
        );
        assert_eq!(chunks[1].removed_repositories, vec![21, 34]);
        assert!(chunks[1].is_last_update);
    }

    #[test]
    fn test_lsp_query_name_and_write_permissions() {
        let cases = [
            (
                LspQuery {
                    request: Some(lsp_query::Request::GetHover(GetHover::default())),
                    ..Default::default()
                },
                ("GetHover", false),
            ),
            (
                LspQuery {
                    request: Some(lsp_query::Request::GetCodeActions(GetCodeActions::default())),
                    ..Default::default()
                },
                ("GetCodeActions", true),
            ),
            (
                LspQuery {
                    request: Some(lsp_query::Request::GetDocumentDiagnostics(
                        GetDocumentDiagnostics::default(),
                    )),
                    ..Default::default()
                },
                ("GetDocumentDiagnostics", false),
            ),
            (
                LspQuery {
                    request: Some(lsp_query::Request::SemanticTokens(SemanticTokens::default())),
                    ..Default::default()
                },
                ("SemanticTokens", false),
            ),
            (
                LspQuery {
                    request: Some(lsp_query::Request::GetDocumentLinks(
                        GetDocumentLinks::default(),
                    )),
                    ..Default::default()
                },
                ("GetDocumentLinks", false),
            ),
            (LspQuery::default(), ("<unknown>", true)),
        ];

        for (query, expected) in cases {
            assert_eq!(query.query_name_and_write_permissions(), expected);
        }
    }
}
