namespace Actrium.Windows;

/// <summary>
/// The single command-to-glyph table for the toolbar icon font. Every
/// codepoint here MUST exist in Segoe MDL2 Assets: the app targets
/// Windows 10 19041, where Segoe Fluent Icons is not installed, so a
/// Fluent-only codepoint would render as tofu once the glyph font stack
/// falls back to MDL2. The harness whitelist encodes that rule — extend it
/// only with codepoints verified against the Segoe MDL2 Assets reference.
/// Harness-linked: this file must stay WinForms-free.
/// </summary>
internal static class BrowserGlyphs
{
    /// <summary>
    /// Glyph point size (UI text is 10 pt; icon strokes read best slightly
    /// larger inside the toolbar buttons).
    /// </summary>
    public const float GlyphPointSize = 11f;

    // Segoe MDL2 Assets codepoints; the trailing names are the official
    // MDL2 glyph names the whitelist is verified against.
    public const char NewTab = '\uE710';             // Add
    public const char TabOverview = '\uE8A9';        // ViewAll
    public const char ToggleSidebar = '\uE8A0';      // OpenPane
    public const char Back = '\uE72B';               // Back
    public const char Forward = '\uE72A';            // Forward
    public const char Reload = '\uE72C';             // Refresh
    public const char Stop = '\uE711';               // Cancel
    public const char BookmarkStar = '\uE734';       // FavoriteStar
    public const char BookmarkStarFilled = '\uE735'; // FavoriteStarFill
    public const char AppMenu = '\uE712';            // More

    /// <summary>
    /// Every toolbar-invocable command mapped to its resting glyph. The
    /// reload button flips to <see cref="Stop"/> while a page loads, and the
    /// bookmark star flips to <see cref="BookmarkStarFilled"/> when the page
    /// is bookmarked; both states stay in this table so no glyph can bypass
    /// the MDL2 whitelist. <see cref="AppMenu"/> is not a command (the
    /// ellipsis opens the app menu), so it lives as a named constant and in
    /// <see cref="All"/> only.
    /// </summary>
    public static IReadOnlyDictionary<BrowserMenuCommand, char> CommandGlyphs { get; } =
        new Dictionary<BrowserMenuCommand, char>
        {
            [BrowserMenuCommand.NewTab] = NewTab,
            [BrowserMenuCommand.ToggleTabOverview] = TabOverview,
            [BrowserMenuCommand.ToggleSidebar] = ToggleSidebar,
            [BrowserMenuCommand.Back] = Back,
            [BrowserMenuCommand.Forward] = Forward,
            [BrowserMenuCommand.Reload] = Reload,
            [BrowserMenuCommand.Stop] = Stop,
            [BrowserMenuCommand.BookmarkPage] = BookmarkStar,
        };

    /// <summary>Every glyph the chrome may render, for exhaustive harness checks.</summary>
    public static IReadOnlyList<char> All { get; } =
    [
        NewTab,
        TabOverview,
        ToggleSidebar,
        Back,
        Forward,
        Reload,
        Stop,
        BookmarkStar,
        BookmarkStarFilled,
        AppMenu,
    ];

    /// <summary>
    /// Resting glyph for a toolbar command. An unmapped command throws:
    /// that is a wiring bug caught at chrome construction, not a state the
    /// UI should soften.
    /// </summary>
    public static char Get(BrowserMenuCommand command)
    {
        if (!CommandGlyphs.TryGetValue(command, out var glyph))
        {
            throw new ArgumentOutOfRangeException(
                nameof(command),
                command,
                "Command has no toolbar glyph.");
        }

        return glyph;
    }
}
