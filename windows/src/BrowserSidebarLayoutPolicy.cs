namespace Actro.Windows;

/// <summary>
/// Pure geometry rules for the vertical tab sidebar: its width is clamped
/// between the DPI-scaled minimum and half the window, and a resize request
/// below the collapse threshold means "fold the sidebar away" instead of
/// leaving an unusable sliver. All inputs and outputs are pixels at the
/// caller's DPI; the 96-DPI base values come from BrowserTheme.Metrics so
/// the sidebar shares the single chrome metrics source.
/// Harness-linked: this file must stay WinForms-free.
/// </summary>
internal static class BrowserSidebarLayoutPolicy
{
    /// <summary>Narrowest usable sidebar (180 px at 96 DPI).</summary>
    public static int MinWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.SidebarMinWidth, dpi);
    }

    /// <summary>Initial sidebar width (240 px at 96 DPI).</summary>
    public static int DefaultWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.SidebarDefaultWidth, dpi);
    }

    /// <summary>
    /// Upper width bound: half the window, but never below the scaled
    /// minimum — on a window too narrow for the 50 percent rule the minimum
    /// wins, so the sidebar stays usable instead of shrinking further.
    /// </summary>
    public static int MaxWidth(int windowWidth, int dpi)
    {
        return Math.Max(MinWidth(dpi), windowWidth / 2);
    }

    /// <summary>Clamps a requested width into the min..max band.</summary>
    public static int ClampWidth(int requestedWidth, int windowWidth, int dpi)
    {
        return Math.Clamp(requestedWidth, MinWidth(dpi), MaxWidth(windowWidth, dpi));
    }

    /// <summary>
    /// Width below which a resize request reads as a collapse gesture:
    /// half the scaled minimum width.
    /// </summary>
    public static int CollapseThreshold(int dpi)
    {
        return MinWidth(dpi) / 2;
    }

    /// <summary>True when a requested width should collapse the sidebar.</summary>
    public static bool ShouldCollapse(int requestedWidth, int dpi)
    {
        return requestedWidth < CollapseThreshold(dpi);
    }
}
