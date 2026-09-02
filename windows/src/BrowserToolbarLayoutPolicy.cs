namespace Actrium.Windows;

/// <summary>
/// Pure geometry for the two-segment toolbar. Structure follows the macOS
/// app: a sidebar-aligned segment carries [+ new tab][tab overview] above
/// the vertical tab sidebar, and the content segment carries the
/// Chrome-ordered nav cluster, the centered width-capped address box and
/// the ellipsis app menu. When the sidebar collapses, the first segment
/// shrinks to a fixed stub wide enough for both segment buttons — one-click
/// new-tab never disappears with the sidebar. All inputs and outputs are
/// pixels at the caller's DPI; 96-DPI base values resolve through
/// <see cref="BrowserTheme.Metrics.Scale"/> so the toolbar shares the
/// single chrome metrics source.
/// Harness-linked: this file must stay WinForms-free.
/// </summary>
internal static class BrowserToolbarLayoutPolicy
{
    /// <summary>Glyph button width at 96 DPI.</summary>
    public const int ButtonWidthBase = 36;

    /// <summary>Gap between adjacent toolbar buttons at 96 DPI.</summary>
    public const int ButtonGapBase = 2;

    /// <summary>Horizontal inset of each segment's outer edge at 96 DPI.</summary>
    public const int EdgePaddingBase = 8;

    /// <summary>
    /// Address box cap at 96 DPI. Wider content segments center the box in
    /// the leftover space instead of stretching it edge to edge.
    /// </summary>
    public const int AddressMaxWidthBase = 720;

    public static int ButtonWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(ButtonWidthBase, dpi);
    }

    public static int ButtonGap(int dpi)
    {
        return BrowserTheme.Metrics.Scale(ButtonGapBase, dpi);
    }

    public static int EdgePadding(int dpi)
    {
        return BrowserTheme.Metrics.Scale(EdgePaddingBase, dpi);
    }

    /// <summary>
    /// Fixed sidebar-segment width while the sidebar is collapsed (88 px at
    /// 96 DPI): room for [+ new tab] plus the overview slot.
    /// </summary>
    public static int CollapsedStubWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.CollapsedSegmentWidth, dpi);
    }

    /// <summary>
    /// Reserved slot at the segment's right edge, kept clear of the address
    /// cluster; the tab-overview button lands here in the overview slice.
    /// </summary>
    public static int OverviewSlotWidth(int dpi)
    {
        return ButtonWidth(dpi);
    }

    public static int AddressMaxWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(AddressMaxWidthBase, dpi);
    }

    /// <summary>Address box width for the space left over by the nav cluster.</summary>
    public static int AddressWidth(int availableWidth, int dpi)
    {
        return Math.Clamp(availableWidth, 0, AddressMaxWidth(dpi));
    }

    /// <summary>
    /// Splits the toolbar into [sidebar segment | content segment].
    /// Expanded, the first segment tracks the sidebar width (never below
    /// the stub); collapsed, it is exactly the stub. A toolbar narrower
    /// than the segment clamps rather than overflowing — transient
    /// mid-layout sizes must not produce negative content widths.
    /// </summary>
    public static BrowserToolbarSegments Segments(
        int sidebarWidth,
        bool sidebarCollapsed,
        int toolbarWidth,
        int dpi)
    {
        var stub = CollapsedStubWidth(dpi);
        var sidebarSegmentWidth = sidebarCollapsed ? stub : Math.Max(stub, sidebarWidth);
        sidebarSegmentWidth = Math.Min(sidebarSegmentWidth, Math.Max(0, toolbarWidth));
        return new BrowserToolbarSegments(
            sidebarSegmentWidth,
            sidebarSegmentWidth,
            Math.Max(0, toolbarWidth - sidebarSegmentWidth));
    }
}

/// <summary>Resolved two-segment split, in pixels at the caller's DPI.</summary>
internal readonly record struct BrowserToolbarSegments(
    int SidebarSegmentWidth,
    int ContentSegmentLeft,
    int ContentSegmentWidth);
