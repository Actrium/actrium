using System.Drawing;
using System.Globalization;
using System.Text;

namespace Actro.Windows;

/// <summary>
/// Shared stylesheet for internal (actro://) documents: one :root block of
/// CSS custom properties built from the light palette plus a
/// prefers-color-scheme override built from the dark palette. Every generated
/// document embeds <see cref="RootCss"/> exactly once, so the 12-color CSS
/// palette is no longer duplicated per document policy. Dark mode reaches the
/// pages through CoreWebView2 Profile.PreferredColorScheme.
/// </summary>
internal static class BrowserDocumentStylePolicy
{
    /// <summary>Focus-ring tint derived from the shared accent.</summary>
    public const string AccentSoft = "rgba(22, 93, 255, 0.16)";

    public static string RootCss { get; } = BuildRootCss();

    public static string UiFontStack { get; } =
        $"\"{BrowserTheme.UiFontFamily}\", system-ui, sans-serif";

    public static string MonoFontStack { get; } =
        string.Join(", ", BrowserTheme.MonoFontFamilies.Select(family => $"\"{family}\"")) + ", monospace";

    public static string FormatColor(Color color)
    {
        return string.Create(
            CultureInfo.InvariantCulture,
            $"#{color.R:x2}{color.G:x2}{color.B:x2}");
    }

    private static string BuildRootCss()
    {
        var css = new StringBuilder();
        css.AppendLine(":root {");
        AppendPalette(css, BrowserTheme.Light);
        css.AppendLine($"  --accent-soft: {AccentSoft};");
        css.AppendLine($"  --ui-font: {UiFontStack};");
        css.AppendLine($"  --mono-font: {MonoFontStack};");
        css.AppendLine("}");
        css.AppendLine("@media (prefers-color-scheme: dark) {");
        css.AppendLine("  :root {");
        AppendPalette(css, BrowserTheme.Dark, indent: "  ");
        css.AppendLine("  }");
        css.Append('}');
        return css.ToString();
    }

    private static void AppendPalette(StringBuilder css, BrowserTheme theme, string indent = "")
    {
        css.AppendLine($"{indent}  color-scheme: {(theme.IsDark ? "dark" : "light")};");
        css.AppendLine($"{indent}  --page-background: {FormatColor(theme.WindowBackground)};");
        css.AppendLine($"{indent}  --page-foreground: {FormatColor(theme.BarForeground)};");
        css.AppendLine($"{indent}  --muted-foreground: {FormatColor(theme.BarForegroundMuted)};");
        css.AppendLine($"{indent}  --disabled-foreground: {FormatColor(theme.DisabledForeground)};");
        css.AppendLine($"{indent}  --accent: {FormatColor(theme.Accent)};");
        css.AppendLine($"{indent}  --card-background: {FormatColor(theme.CardBackground)};");
        css.AppendLine($"{indent}  --border: {FormatColor(theme.Border)};");
        css.AppendLine($"{indent}  --input-background: {FormatColor(theme.InputBackground)};");
        css.AppendLine($"{indent}  --hover-background: {FormatColor(theme.ControlHover)};");
    }
}
