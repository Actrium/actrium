using System.Drawing;

namespace Actrium.Windows;

/// <summary>
/// The single source for chrome palette, typography and pixel metrics.
/// Pure data so the test harness can exercise it on any host: colors are
/// <see cref="Color"/> values (System.Drawing.Primitives), fonts are family
/// names plus point sizes (Font objects are constructed and cached UI-side
/// by BrowserThemeManager), and metrics are 96-DPI base values resolved
/// through <see cref="Metrics.Scale"/>.
/// </summary>
internal sealed class BrowserTheme
{
    private BrowserTheme(
        string name,
        bool isDark,
        Color windowBackground,
        Color barBackground,
        Color barForeground,
        Color barForegroundMuted,
        Color controlHover,
        Color controlPressed,
        Color border,
        Color accent,
        Color sidebarBackground,
        Color tabActiveBackground,
        Color tabActiveForeground,
        Color inputBackground,
        Color inputForeground,
        Color disabledForeground,
        Color cardBackground,
        Color statusBackground)
    {
        Name = name;
        IsDark = isDark;
        WindowBackground = windowBackground;
        BarBackground = barBackground;
        BarForeground = barForeground;
        BarForegroundMuted = barForegroundMuted;
        ControlHover = controlHover;
        ControlPressed = controlPressed;
        Border = border;
        Accent = accent;
        SidebarBackground = sidebarBackground;
        TabActiveBackground = tabActiveBackground;
        TabActiveForeground = tabActiveForeground;
        InputBackground = inputBackground;
        InputForeground = inputForeground;
        DisabledForeground = disabledForeground;
        CardBackground = cardBackground;
        StatusBackground = statusBackground;
        Colors = new Dictionary<string, Color>(StringComparer.Ordinal)
        {
            [nameof(WindowBackground)] = WindowBackground,
            [nameof(BarBackground)] = BarBackground,
            [nameof(BarForeground)] = BarForeground,
            [nameof(BarForegroundMuted)] = BarForegroundMuted,
            [nameof(ControlHover)] = ControlHover,
            [nameof(ControlPressed)] = ControlPressed,
            [nameof(Border)] = Border,
            [nameof(Accent)] = Accent,
            [nameof(SidebarBackground)] = SidebarBackground,
            [nameof(TabActiveBackground)] = TabActiveBackground,
            [nameof(TabActiveForeground)] = TabActiveForeground,
            [nameof(InputBackground)] = InputBackground,
            [nameof(InputForeground)] = InputForeground,
            [nameof(DisabledForeground)] = DisabledForeground,
            [nameof(CardBackground)] = CardBackground,
            [nameof(StatusBackground)] = StatusBackground,
        };
    }

    /// <summary>Light palette; the pre-theme chrome literals live on here.</summary>
    public static BrowserTheme Light { get; } = new(
        name: "light",
        isDark: false,
        windowBackground: Color.FromArgb(246, 247, 249),
        barBackground: Color.FromArgb(246, 247, 249),
        barForeground: Color.FromArgb(18, 20, 23),
        barForegroundMuted: Color.FromArgb(89, 97, 109),
        controlHover: Color.FromArgb(233, 237, 242),
        controlPressed: Color.FromArgb(221, 227, 234),
        border: Color.FromArgb(200, 208, 220),
        accent: Color.FromArgb(22, 93, 255),
        sidebarBackground: Color.FromArgb(232, 235, 240),
        tabActiveBackground: Color.FromArgb(255, 255, 255),
        tabActiveForeground: Color.FromArgb(18, 20, 23),
        inputBackground: Color.FromArgb(255, 255, 255),
        inputForeground: Color.FromArgb(18, 20, 23),
        disabledForeground: Color.FromArgb(154, 163, 175),
        cardBackground: Color.FromArgb(255, 255, 255),
        statusBackground: Color.FromArgb(250, 251, 252));

    /// <summary>Dark palette, tuned to Chrome/Edge dark chrome tones.</summary>
    public static BrowserTheme Dark { get; } = new(
        name: "dark",
        isDark: true,
        windowBackground: Color.FromArgb(32, 33, 36),
        barBackground: Color.FromArgb(41, 42, 45),
        barForeground: Color.FromArgb(232, 234, 237),
        barForegroundMuted: Color.FromArgb(154, 160, 166),
        controlHover: Color.FromArgb(60, 64, 67),
        controlPressed: Color.FromArgb(73, 76, 80),
        border: Color.FromArgb(95, 99, 104),
        // The accent is deliberately shared with the light palette and the
        // internal document stylesheet.
        accent: Color.FromArgb(22, 93, 255),
        sidebarBackground: Color.FromArgb(24, 25, 27),
        tabActiveBackground: Color.FromArgb(60, 64, 67),
        tabActiveForeground: Color.FromArgb(232, 234, 237),
        inputBackground: Color.FromArgb(48, 49, 52),
        inputForeground: Color.FromArgb(232, 234, 237),
        disabledForeground: Color.FromArgb(112, 117, 122),
        cardBackground: Color.FromArgb(41, 42, 45),
        statusBackground: Color.FromArgb(41, 42, 45));

    public string Name { get; }

    public bool IsDark { get; }

    public Color WindowBackground { get; }

    public Color BarBackground { get; }

    public Color BarForeground { get; }

    public Color BarForegroundMuted { get; }

    public Color ControlHover { get; }

    public Color ControlPressed { get; }

    public Color Border { get; }

    public Color Accent { get; }

    public Color SidebarBackground { get; }

    public Color TabActiveBackground { get; }

    public Color TabActiveForeground { get; }

    public Color InputBackground { get; }

    public Color InputForeground { get; }

    public Color DisabledForeground { get; }

    public Color CardBackground { get; }

    public Color StatusBackground { get; }

    /// <summary>Every surface role by name, for exhaustive palette checks.</summary>
    public IReadOnlyDictionary<string, Color> Colors { get; }

    /// <summary>UI font family; sizes are points, so they stay DPI-correct.</summary>
    public static string UiFontFamily => "Segoe UI";

    public static float UiFontPointSize => 10f;

    /// <summary>Monospace stack for internal documents, best match first.</summary>
    public static IReadOnlyList<string> MonoFontFamilies { get; } = ["Cascadia Mono", "Consolas"];

    /// <summary>Icon-glyph font stack, best match first.</summary>
    public static IReadOnlyList<string> GlyphFontFamilies { get; } = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];

    /// <summary>
    /// Chrome pixel metrics stored at the 96-DPI base. Consumers must resolve
    /// them through <see cref="Scale"/> with the control's current DPI.
    /// </summary>
    public static class Metrics
    {
        public const int ToolbarHeight = 44;
        public const int TabRowHeight = 32;
        public const int SidebarMinWidth = 180;
        public const int SidebarDefaultWidth = 240;
        public const int FindBarHeight = 40;
        public const int CollapsedSegmentWidth = 88;
        public const int InputHeight = 28;
        public const int BorderThickness = 1;

        /// <summary>Scales a 96-DPI base value to the given DPI, rounding half away from zero.</summary>
        public static int Scale(int baseValue, int dpi)
        {
            return (int)Math.Round(baseValue * dpi / 96.0, MidpointRounding.AwayFromZero);
        }
    }
}
