namespace Actro.Windows;

/// <summary>
/// WinForms-free keyboard shortcut: modifier flags plus a canonical key
/// token ("T", "F11", "Left", "Oemplus", ...). BrowserAppMenu owns the
/// token-to-Keys map used for shortcut display; an unmapped token throws
/// there at startup so a typo cannot ship as a silently missing hint.
/// </summary>
internal sealed record BrowserShortcut(bool Control, bool Shift, bool Alt, string Key);

/// <summary>
/// A secondary shortcut for a command that already has (or deliberately
/// lacks) a primary one, e.g. F5 for reload. Aliases are first-class model
/// data so the harness can prove the legacy shortcut table survived the
/// menu rework.
/// </summary>
internal sealed record BrowserMenuShortcutAlias(BrowserShortcut Shortcut, BrowserMenuCommand Command);

/// <summary>
/// One node of the compact app menu tree. Leaf items carry a command (plus
/// an optional argument for per-item data such as a search engine id);
/// submenu headers carry only children. Text comes from the centralized
/// string table via <see cref="StringKey"/>, except for items whose label
/// is runtime data (search engine display names) which use
/// <see cref="LiteralText"/>. Harness-linked: WinForms-free.
/// </summary>
internal sealed record BrowserMenuItem
{
    public BrowserMenuCommand? Command { get; init; }

    public string? Argument { get; init; }

    public string? StringKey { get; init; }

    public string? LiteralText { get; init; }

    public BrowserShortcut? Shortcut { get; init; }

    /// <summary>
    /// Display-only shortcut hint for keys that cannot be model shortcuts,
    /// e.g. "Esc" on Stop (owned by BrowserEscapePolicy) and the F3 find
    /// aliases.
    /// </summary>
    public string? ShortcutDisplayOverride { get; init; }

    public bool IsSeparator { get; init; }

    public bool IsCheckable { get; init; }

    public IReadOnlyList<BrowserMenuItem> Children { get; init; } = [];
}

/// <summary>
/// The compact app menu as pure data: item tree, shortcut aliases and the
/// menu-opener key set. BrowserAppMenu renders this into a themed
/// ContextMenuStrip; MainForm.ProcessCmdKey owns the actual key handling.
/// Structure follows the macOS app's command surface; presentation follows
/// the Chrome/Edge three-dot menu. Harness-linked: WinForms-free.
/// </summary>
internal static class BrowserMenuModel
{
    /// <summary>
    /// Secondary shortcuts preserved from the classic menu era: function-key
    /// and numpad variants that must keep working alongside the primaries.
    /// </summary>
    public static IReadOnlyList<BrowserMenuShortcutAlias> Aliases { get; } =
    [
        new(Plain("F5"), BrowserMenuCommand.Reload),
        new(CtrlShift("Oemplus"), BrowserMenuCommand.ZoomIn),
        new(Ctrl("Add"), BrowserMenuCommand.ZoomIn),
        new(Ctrl("Subtract"), BrowserMenuCommand.ZoomOut),
        new(Ctrl("NumPad0"), BrowserMenuCommand.ZoomReset),
        new(Plain("F3"), BrowserMenuCommand.FindNext),
        new(Shift("F3"), BrowserMenuCommand.FindPrevious),
    ];

    /// <summary>
    /// Keys that open the compact app menu itself (Chrome/Edge convention),
    /// replacing the classic menu bar's Alt mnemonics.
    /// </summary>
    public static IReadOnlyList<BrowserShortcut> MenuOpenShortcuts { get; } =
    [
        Alt("E"),
        Alt("F"),
        Plain("F10"),
    ];

    /// <summary>
    /// Builds the menu tree. Rebuilt per menu instance because the search
    /// engine items come from the Rust core catalog: one checkable item per
    /// engine with the engine id as the command argument.
    /// </summary>
    public static IReadOnlyList<BrowserMenuItem> BuildItems()
    {
        return
        [
            Item(BrowserMenuCommand.NewTab, "menu.newTab", Ctrl("T")),
            Item(BrowserMenuCommand.NewWindow, "menu.newWindow", Ctrl("N")),
            Item(BrowserMenuCommand.NewPrivateWindow, "menu.newPrivateWindow", CtrlShift("N")),
            Separator(),
            Item(BrowserMenuCommand.CloseTab, "menu.closeTab", Ctrl("W")),
            Item(BrowserMenuCommand.ReopenClosedTab, "menu.reopenClosedTab", CtrlShift("T")),
            Item(BrowserMenuCommand.DuplicateTab, "menu.duplicateTab"),
            Separator(),
            Item(BrowserMenuCommand.Back, "menu.back", Alt("Left")),
            Item(BrowserMenuCommand.Forward, "menu.forward", Alt("Right")),
            Item(BrowserMenuCommand.Home, "menu.home", Alt("Home")),
            Item(BrowserMenuCommand.Reload, "menu.reload", Ctrl("R")),
            // Escape is not a model shortcut: BrowserEscapePolicy owns its
            // precedence, the menu only shows the hint.
            Item(BrowserMenuCommand.Stop, "menu.stop", shortcutDisplayOverride: "Esc"),
            Separator(),
            Item(BrowserMenuCommand.ZoomIn, "menu.zoomIn", Ctrl("Oemplus")),
            Item(BrowserMenuCommand.ZoomOut, "menu.zoomOut", Ctrl("OemMinus")),
            Item(BrowserMenuCommand.ZoomReset, "menu.zoomReset", Ctrl("D0")),
            Item(BrowserMenuCommand.FullScreen, "menu.fullScreen", Plain("F11")),
            // Ctrl+Shift+, is the Edge vertical-tabs chord.
            Item(BrowserMenuCommand.ToggleSidebar, "menu.toggleSidebar", CtrlShift("OemComma")),
            Separator(),
            Item(BrowserMenuCommand.Find, "menu.find", Ctrl("F")),
            Item(BrowserMenuCommand.FindNext, "menu.findNext", shortcutDisplayOverride: "F3"),
            Item(BrowserMenuCommand.FindPrevious, "menu.findPrevious", shortcutDisplayOverride: "Shift+F3"),
            Separator(),
            Item(BrowserMenuCommand.ShowHistory, "menu.showHistory", Ctrl("H")),
            Item(BrowserMenuCommand.BookmarkPage, "menu.bookmarkPage", Ctrl("D"), isCheckable: true),
            Item(BrowserMenuCommand.ShowBookmarks, "menu.showBookmarks", CtrlShift("O")),
            Separator(),
            Submenu(
                "menu.page",
                Item(BrowserMenuCommand.OpenFile, "menu.openFile", Ctrl("O")),
                Item(BrowserMenuCommand.CopyAddress, "menu.copyAddress"),
                Item(BrowserMenuCommand.OpenInDefaultBrowser, "menu.openInDefaultBrowser"),
                Item(BrowserMenuCommand.SavePdf, "menu.savePdf", Ctrl("S")),
                Item(BrowserMenuCommand.Print, "menu.print", Ctrl("P"))),
            Submenu(
                "menu.downloads",
                Item(BrowserMenuCommand.OpenDownloadsFolder, "menu.openDownloadsFolder", Ctrl("J")),
                Item(BrowserMenuCommand.OpenLastDownload, "menu.openLastDownload"),
                Item(BrowserMenuCommand.RevealLastDownload, "menu.revealLastDownload")),
            Submenu(
                "menu.moreTools",
                Item(BrowserMenuCommand.FocusAddress, "menu.focusAddress", Ctrl("L")),
                Item(BrowserMenuCommand.DevTools, "menu.devTools", CtrlShift("I"))),
            Separator(),
            Submenu("menu.searchEngine", BuildSearchEngineItems()),
            Item(BrowserMenuCommand.RestoreSessionToggle, "menu.restoreSession", isCheckable: true),
            Item(BrowserMenuCommand.BlockAdsToggle, "menu.blockAds", isCheckable: true),
            Submenu(
                "menu.sync",
                Item(BrowserMenuCommand.ConfigureSync, "menu.configureSync"),
                Item(BrowserMenuCommand.SyncNow, "menu.syncNow"),
                Item(BrowserMenuCommand.SyncStatus, "menu.syncStatus")),
            Item(BrowserMenuCommand.ClearBrowsingData, "menu.clearBrowsingData", CtrlShift("Delete")),
            Separator(),
            Item(BrowserMenuCommand.Exit, "menu.exit", Alt("F4")),
        ];
    }

    /// <summary>Depth-first traversal over items and their submenus.</summary>
    public static IEnumerable<BrowserMenuItem> Flatten(IEnumerable<BrowserMenuItem> items)
    {
        foreach (var item in items)
        {
            yield return item;
            foreach (var child in Flatten(item.Children))
            {
                yield return child;
            }
        }
    }

    private static BrowserMenuItem[] BuildSearchEngineItems()
    {
        return BrowserSearchEnginePolicy.Engines
            .Select(engine => new BrowserMenuItem
            {
                Command = BrowserMenuCommand.SelectSearchEngine,
                Argument = engine.Id,
                LiteralText = engine.DisplayName,
                IsCheckable = true,
            })
            .ToArray();
    }

    private static BrowserMenuItem Item(
        BrowserMenuCommand command,
        string stringKey,
        BrowserShortcut? shortcut = null,
        string? shortcutDisplayOverride = null,
        bool isCheckable = false)
    {
        return new BrowserMenuItem
        {
            Command = command,
            StringKey = stringKey,
            Shortcut = shortcut,
            ShortcutDisplayOverride = shortcutDisplayOverride,
            IsCheckable = isCheckable,
        };
    }

    private static BrowserMenuItem Submenu(string stringKey, params BrowserMenuItem[] children)
    {
        return new BrowserMenuItem
        {
            StringKey = stringKey,
            Children = children,
        };
    }

    private static BrowserMenuItem Separator()
    {
        return new BrowserMenuItem { IsSeparator = true };
    }

    private static BrowserShortcut Ctrl(string key)
    {
        return new BrowserShortcut(Control: true, Shift: false, Alt: false, key);
    }

    private static BrowserShortcut CtrlShift(string key)
    {
        return new BrowserShortcut(Control: true, Shift: true, Alt: false, key);
    }

    private static BrowserShortcut Shift(string key)
    {
        return new BrowserShortcut(Control: false, Shift: true, Alt: false, key);
    }

    private static BrowserShortcut Alt(string key)
    {
        return new BrowserShortcut(Control: false, Shift: false, Alt: true, key);
    }

    private static BrowserShortcut Plain(string key)
    {
        return new BrowserShortcut(Control: false, Shift: false, Alt: false, key);
    }
}
