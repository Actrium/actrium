namespace Actro.Windows;

/// <summary>
/// Every user-invocable chrome command, shared by the compact app menu and
/// the keyboard shortcut router so command behavior cannot fork between
/// input paths. Commands that need per-item data (currently only
/// <see cref="SelectSearchEngine"/>) receive it through the string argument
/// channel of the menu model and the MainForm command router. The enum
/// grows together with the features: sidebar and tab-overview toggles join
/// in their own slices, so no dead or disabled menu items ever ship.
/// Harness-linked: this file must stay WinForms-free.
/// </summary>
internal enum BrowserMenuCommand
{
    NewTab,
    NewWindow,
    NewPrivateWindow,
    CloseTab,
    ReopenClosedTab,
    DuplicateTab,
    OpenFile,
    CopyAddress,
    OpenInDefaultBrowser,
    SavePdf,
    Print,
    Exit,
    Back,
    Forward,
    Home,
    ShowHistory,
    BookmarkPage,
    ShowBookmarks,
    Reload,
    Stop,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    FullScreen,
    Find,
    FindNext,
    FindPrevious,
    FocusAddress,
    DevTools,
    OpenDownloadsFolder,
    OpenLastDownload,
    RevealLastDownload,
    SelectSearchEngine,
    RestoreSessionToggle,
    BlockAdsToggle,
    ConfigureSync,
    SyncNow,
    SyncStatus,
    ClearBrowsingData,
}
