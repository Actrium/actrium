using System.Windows.Forms;

namespace Actro.Windows;

/// <summary>
/// Compact application menu (Chrome/Edge three-dot style): a themed
/// ContextMenuStrip rendered from the pure BrowserMenuModel tree. This
/// class only presents commands — clicks surface as
/// <see cref="CommandRequested"/> and land in MainForm.ExecuteCommand, and
/// keyboard shortcuts are handled solely by MainForm.ProcessCmdKey; the
/// token-to-Keys map here exists for shortcut display strings only, and an
/// unmapped token throws at startup instead of shipping a wrong hint.
/// Item ForeColor is managed explicitly per enabled state because the
/// renderer color table alone leaves system-colored text on dark surfaces.
/// </summary>
internal sealed class BrowserAppMenu : ContextMenuStrip
{
    private static readonly IReadOnlyDictionary<string, Keys> KeyTokenMap =
        new Dictionary<string, Keys>(StringComparer.Ordinal)
        {
            ["D"] = Keys.D,
            ["E"] = Keys.E,
            ["F"] = Keys.F,
            ["H"] = Keys.H,
            ["I"] = Keys.I,
            ["J"] = Keys.J,
            ["L"] = Keys.L,
            ["N"] = Keys.N,
            ["O"] = Keys.O,
            ["P"] = Keys.P,
            ["R"] = Keys.R,
            ["S"] = Keys.S,
            ["T"] = Keys.T,
            ["W"] = Keys.W,
            ["D0"] = Keys.D0,
            ["F3"] = Keys.F3,
            ["F4"] = Keys.F4,
            ["F5"] = Keys.F5,
            ["F10"] = Keys.F10,
            ["F11"] = Keys.F11,
            ["Left"] = Keys.Left,
            ["Right"] = Keys.Right,
            ["Home"] = Keys.Home,
            ["Delete"] = Keys.Delete,
            ["Oemplus"] = Keys.Oemplus,
            ["OemMinus"] = Keys.OemMinus,
            ["OemComma"] = Keys.Oemcomma,
            ["Add"] = Keys.Add,
            ["Subtract"] = Keys.Subtract,
            ["NumPad0"] = Keys.NumPad0,
        };

    private readonly Dictionary<BrowserMenuCommand, ToolStripMenuItem> commandItems = [];
    private readonly Dictionary<string, ToolStripMenuItem> searchEngineItems = [];
    private readonly List<ToolStripMenuItem> allItems = [];
    private BrowserTheme theme = BrowserTheme.Light;

    public event EventHandler<BrowserMenuCommandEventArgs>? CommandRequested;

    public BrowserAppMenu()
    {
        foreach (var model in BrowserMenuModel.BuildItems())
        {
            Items.Add(BuildItem(model));
        }

        // Fail fast on any unmapped alias or opener token as well: these
        // never render a display string, so validate them here where menu
        // construction already runs at startup.
        foreach (var alias in BrowserMenuModel.Aliases)
        {
            _ = ToKeys(alias.Shortcut);
        }

        foreach (var opener in BrowserMenuModel.MenuOpenShortcuts)
        {
            _ = ToKeys(opener);
        }

        ApplyTheme(BrowserThemeManager.Instance.Current);
        SetNavigationState(
            canGoBack: false,
            canGoForward: false,
            isLoading: false,
            hasActiveTab: false,
            canCopyPageAddress: false,
            canOpenInDefaultBrowser: false,
            canReopenClosedTab: false,
            canBookmarkPage: false,
            isActivePageBookmarked: false,
            canOpenLastDownload: false,
            canZoomIn: false,
            canZoomOut: false,
            canResetZoom: false);
        SetSearchEngine(BrowserSearchEnginePolicy.DefaultEngineId);
        SetRestorePreviousSession(true);
        SetContentBlockingEnabled(BrowserContentBlockingPolicy.DefaultIsEnabled);
    }

    /// <summary>
    /// Model shortcut to WinForms Keys, used for display formatting here
    /// and for shortcut routing tables nowhere — actual handling lives in
    /// MainForm.ProcessCmdKey. Throws on an unmapped token.
    /// </summary>
    public static Keys ToKeys(BrowserShortcut shortcut)
    {
        if (!KeyTokenMap.TryGetValue(shortcut.Key, out var key))
        {
            throw new InvalidOperationException(
                $"Unmapped menu shortcut key token: {shortcut.Key}");
        }

        if (shortcut.Control)
        {
            key |= Keys.Control;
        }

        if (shortcut.Shift)
        {
            key |= Keys.Shift;
        }

        if (shortcut.Alt)
        {
            key |= Keys.Alt;
        }

        return key;
    }

    /// <summary>Human-readable shortcut hint (Chrome style: "Ctrl+Shift+Del", "Ctrl++").</summary>
    public static string FormatShortcut(BrowserShortcut shortcut)
    {
        // Validates the token even though the friendly text is hand-mapped:
        // a shortcut that cannot be expressed as Keys must fail at startup.
        _ = ToKeys(shortcut);
        var keyText = shortcut.Key switch
        {
            "Oemplus" => "+",
            "OemMinus" => "-",
            "OemComma" => ",",
            "D0" => "0",
            "Delete" => "Del",
            "Add" => "Num +",
            "Subtract" => "Num -",
            "NumPad0" => "Num 0",
            _ => shortcut.Key,
        };
        return (shortcut.Control ? "Ctrl+" : string.Empty)
            + (shortcut.Shift ? "Shift+" : string.Empty)
            + (shortcut.Alt ? "Alt+" : string.Empty)
            + keyText;
    }

    /// <summary>Applies palette, typography and per-item fore colors in one pass.</summary>
    public void ApplyTheme(BrowserTheme theme)
    {
        this.theme = theme;
        BackColor = theme.CardBackground;
        Font = BrowserThemeManager.Instance.GetUiFont(DeviceDpi);
        Renderer = new BrowserToolStripRenderer(theme);
        foreach (var item in allItems)
        {
            ApplyItemColors(item);
        }
    }

    public void SetSearchEngine(string engineId)
    {
        var normalizedEngineId = BrowserSearchEnginePolicy.NormalizeEngineId(engineId);
        foreach (var (candidateEngineId, item) in searchEngineItems)
        {
            item.Checked = candidateEngineId == normalizedEngineId;
        }
    }

    public void SetRestorePreviousSession(bool isEnabled)
    {
        SetChecked(BrowserMenuCommand.RestoreSessionToggle, isEnabled);
    }

    public void SetContentBlockingEnabled(bool isEnabled)
    {
        SetChecked(BrowserMenuCommand.BlockAdsToggle, isEnabled);
    }

    public void SetNavigationState(
        bool canGoBack,
        bool canGoForward,
        bool isLoading,
        bool hasActiveTab,
        bool canCopyPageAddress,
        bool canOpenInDefaultBrowser,
        bool canReopenClosedTab,
        bool canBookmarkPage,
        bool isActivePageBookmarked,
        bool canOpenLastDownload,
        bool canZoomIn,
        bool canZoomOut,
        bool canResetZoom)
    {
        SetEnabled(BrowserMenuCommand.CloseTab, hasActiveTab);
        SetEnabled(BrowserMenuCommand.ReopenClosedTab, canReopenClosedTab);
        SetEnabled(BrowserMenuCommand.DuplicateTab, hasActiveTab);
        SetEnabled(BrowserMenuCommand.OpenFile, hasActiveTab);
        SetEnabled(BrowserMenuCommand.CopyAddress, canCopyPageAddress);
        SetEnabled(BrowserMenuCommand.OpenInDefaultBrowser, canOpenInDefaultBrowser);
        SetEnabled(BrowserMenuCommand.SavePdf, hasActiveTab);
        SetEnabled(BrowserMenuCommand.Print, hasActiveTab);
        SetEnabled(BrowserMenuCommand.Back, canGoBack);
        SetEnabled(BrowserMenuCommand.Forward, canGoForward);
        SetEnabled(BrowserMenuCommand.Home, hasActiveTab);
        SetEnabled(BrowserMenuCommand.ShowHistory, hasActiveTab);
        SetEnabled(BrowserMenuCommand.BookmarkPage, canBookmarkPage);
        SetChecked(BrowserMenuCommand.BookmarkPage, canBookmarkPage && isActivePageBookmarked);
        SetEnabled(BrowserMenuCommand.ShowBookmarks, hasActiveTab);
        SetEnabled(BrowserMenuCommand.Reload, hasActiveTab);
        SetEnabled(BrowserMenuCommand.Stop, hasActiveTab && isLoading);
        SetEnabled(BrowserMenuCommand.ZoomIn, hasActiveTab && canZoomIn);
        SetEnabled(BrowserMenuCommand.ZoomOut, hasActiveTab && canZoomOut);
        SetEnabled(BrowserMenuCommand.ZoomReset, hasActiveTab && canResetZoom);
        SetEnabled(BrowserMenuCommand.FullScreen, hasActiveTab);
        SetEnabled(BrowserMenuCommand.Find, hasActiveTab);
        SetEnabled(BrowserMenuCommand.FindNext, hasActiveTab);
        SetEnabled(BrowserMenuCommand.FindPrevious, hasActiveTab);
        SetEnabled(BrowserMenuCommand.FocusAddress, hasActiveTab);
        SetEnabled(BrowserMenuCommand.DevTools, hasActiveTab);
        SetEnabled(BrowserMenuCommand.OpenLastDownload, canOpenLastDownload);
        SetEnabled(BrowserMenuCommand.RevealLastDownload, canOpenLastDownload);
        SetEnabled(BrowserMenuCommand.ClearBrowsingData, hasActiveTab);
    }

    private ToolStripItem BuildItem(BrowserMenuItem model)
    {
        if (model.IsSeparator)
        {
            return new ToolStripSeparator();
        }

        var item = new ToolStripMenuItem(ResolveText(model));
        allItems.Add(item);

        if (model.Children.Count > 0)
        {
            foreach (var child in model.Children)
            {
                item.DropDownItems.Add(BuildItem(child));
            }

            return item;
        }

        if (model.Shortcut is BrowserShortcut shortcut)
        {
            item.ShowShortcutKeys = true;
            item.ShortcutKeyDisplayString = FormatShortcut(shortcut);
        }
        else if (model.ShortcutDisplayOverride is string displayOverride)
        {
            item.ShowShortcutKeys = true;
            item.ShortcutKeyDisplayString = displayOverride;
        }
        else
        {
            item.ShowShortcutKeys = false;
        }

        if (model.Command is BrowserMenuCommand command)
        {
            var argument = model.Argument;
            item.Click += (_, _) =>
                CommandRequested?.Invoke(this, new BrowserMenuCommandEventArgs(command, argument));

            if (command == BrowserMenuCommand.SelectSearchEngine && argument is not null)
            {
                searchEngineItems[argument] = item;
            }
            else
            {
                commandItems[command] = item;
            }
        }

        return item;
    }

    private static string ResolveText(BrowserMenuItem model)
    {
        return model.StringKey is string stringKey
            ? BrowserStrings.Get(stringKey)
            : model.LiteralText ?? string.Empty;
    }

    private void SetEnabled(BrowserMenuCommand command, bool isEnabled)
    {
        if (commandItems.TryGetValue(command, out var item))
        {
            item.Enabled = isEnabled;
            ApplyItemColors(item);
        }
    }

    private void SetChecked(BrowserMenuCommand command, bool isChecked)
    {
        if (commandItems.TryGetValue(command, out var item))
        {
            item.Checked = isChecked;
        }
    }

    private void ApplyItemColors(ToolStripMenuItem item)
    {
        item.BackColor = theme.CardBackground;
        item.ForeColor = item.Enabled ? theme.BarForeground : theme.DisabledForeground;
    }
}

internal sealed class BrowserMenuCommandEventArgs : EventArgs
{
    public BrowserMenuCommandEventArgs(BrowserMenuCommand command, string? argument)
    {
        Command = command;
        Argument = argument;
    }

    public BrowserMenuCommand Command { get; }

    public string? Argument { get; }
}
