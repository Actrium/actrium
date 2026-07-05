namespace Actro.Windows;

/// <summary>
/// The action Escape performs, ordered by precedence. Escape cannot be a
/// menu shortcut (ToolStripMenuItem.ShortcutKeys rejects modifierless
/// values at assignment time), so its behavior historically hid inside
/// ProcessCmdKey where the ordering could regress silently. This pure
/// policy makes the precedence explicit and harness-testable.
/// </summary>
internal enum BrowserEscapeAction
{
    None,
    CloseFindBar,
    ExitFullScreen,
    Stop,
}

/// <summary>
/// Resolves what Escape should do for the current chrome state: close the
/// find bar first, then leave page or browser full screen, then stop the
/// loading page, otherwise nothing. The overview parameter joins in the
/// tab-overview slice at the highest precedence. Harness-linked:
/// WinForms-free.
/// </summary>
internal static class BrowserEscapePolicy
{
    public static BrowserEscapeAction Resolve(
        bool findBarVisible,
        bool isPageFullScreen,
        bool isBrowserFullScreen,
        bool isLoading)
    {
        if (findBarVisible)
        {
            return BrowserEscapeAction.CloseFindBar;
        }

        if (isPageFullScreen || isBrowserFullScreen)
        {
            return BrowserEscapeAction.ExitFullScreen;
        }

        return isLoading ? BrowserEscapeAction.Stop : BrowserEscapeAction.None;
    }
}
