using System.Drawing;
using System.Security;
using Microsoft.Win32;

namespace Actro.Windows;

/// <summary>
/// UI-side theme owner: reads the Windows personalization registry value,
/// resolves it through BrowserThemeSelectionPolicy, and raises
/// <see cref="Changed"/> only when the resolved theme actually differs.
/// WM_SETTINGCHANGE arrives once per top-level window, so the
/// compare-and-raise is what keeps N open windows from triggering N
/// redundant ApplyTheme cascades and DwmSetWindowAttribute calls.
/// Also owns the per-(family, size, dpi) Font cache; BrowserTheme itself
/// carries only font family names and point sizes so it stays testable
/// off-Windows.
/// </summary>
internal sealed class BrowserThemeManager
{
    private const string PersonalizeKeyPath =
        @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    private const string AppsUseLightThemeValueName = "AppsUseLightTheme";

    private readonly object fontCacheLock = new();
    private readonly Dictionary<(string Family, float PointSize, int Dpi), Font> fontCache = [];

    private BrowserThemeManager()
    {
        Current = BrowserThemeSelectionPolicy.Resolve(ReadAppsUseLightTheme());
    }

    public static BrowserThemeManager Instance { get; } = new();

    public BrowserTheme Current { get; private set; }

    /// <summary>
    /// Raised only on an actual theme change. Every subscribing form must
    /// unsubscribe on FormClosed to avoid keeping closed windows alive for
    /// the whole multi-window application lifetime.
    /// </summary>
    public event EventHandler? Changed;

    /// <summary>Re-reads the system setting; raises <see cref="Changed"/> only on a change.</summary>
    public void RefreshFromSystem()
    {
        var resolved = BrowserThemeSelectionPolicy.Resolve(ReadAppsUseLightTheme());
        if (ReferenceEquals(resolved, Current))
        {
            return;
        }

        Current = resolved;
        Changed?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>Cached UI font for the current theme typography at the given DPI.</summary>
    public Font GetUiFont(int dpi)
    {
        return GetFont(BrowserTheme.UiFontFamily, BrowserTheme.UiFontPointSize, dpi);
    }

    /// <summary>
    /// Font cache keyed by family, point size and DPI. Fonts are specified in
    /// points (DPI-correct under GDI); the DPI key keeps per-monitor variants
    /// apart once mixed-DPI window moves start re-measuring controls.
    /// </summary>
    public Font GetFont(string family, float pointSize, int dpi)
    {
        var key = (family, pointSize, dpi);
        lock (fontCacheLock)
        {
            if (fontCache.TryGetValue(key, out var cached))
            {
                return cached;
            }

            var font = new Font(family, pointSize, FontStyle.Regular, GraphicsUnit.Point);
            fontCache[key] = font;
            return font;
        }
    }

    private static int? ReadAppsUseLightTheme()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(PersonalizeKeyPath);
            return key?.GetValue(AppsUseLightThemeValueName) as int?;
        }
        catch (SecurityException)
        {
            // Unreadable personalization hive: fall back to the Windows
            // default (light) rather than failing chrome construction.
            return null;
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
    }
}
