namespace Actro.Windows;

/// <summary>
/// Maps the Windows personalization setting to a theme. Pure so the harness
/// can pin the mapping; the registry read itself lives in BrowserThemeManager.
/// </summary>
internal static class BrowserThemeSelectionPolicy
{
    /// <summary>
    /// Resolves the theme from the HKCU AppsUseLightTheme value. Dark only for
    /// an explicit 0; a missing key (null) or any other value means Light,
    /// matching the Windows default. A user override slot can be added here
    /// later without touching callers.
    /// </summary>
    public static BrowserTheme Resolve(int? appsUseLightTheme)
    {
        return appsUseLightTheme == 0 ? BrowserTheme.Dark : BrowserTheme.Light;
    }
}
