using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace Actro.Windows;

/// <summary>
/// Owner-drawn vertical tab sidebar — the app's identity surface, replacing
/// the horizontal tab strip. One row per tab in creation order (so the
/// Ctrl+1..9 index semantics stay stable): favicon with a letter-avatar
/// fallback, title with the loading " ..." marker, a close button on the
/// hovered row, middle-click close, and an accent edge on the active row.
/// A persistent "+ New tab" row sits at the bottom of the list. Exposes the
/// same contract members as the strip it replaces (NewTabRequested,
/// TabSelected, TabCloseRequested, UpdateTabs); tab state stays in MainForm
/// and width/collapse geometry lives in BrowserSidebarLayoutPolicy.
/// </summary>
internal sealed class BrowserTabSidebar : UserControl
{
    private readonly List<BrowserTabHeader> tabs = [];
    private Guid? activeTabId;
    private BrowserTheme theme = BrowserTheme.Light;
    private int scrollOffset;
    private int hoverRowIndex = -1;
    private bool hoverOnCloseButton;

    public event EventHandler? NewTabRequested;
    public event EventHandler<BrowserTabRequestedEventArgs>? TabSelected;
    public event EventHandler<BrowserTabRequestedEventArgs>? TabCloseRequested;

    public BrowserTabSidebar()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        Dock = DockStyle.Fill;
        SetStyle(
            ControlStyles.AllPaintingInWmPaint
            | ControlStyles.OptimizedDoubleBuffer
            | ControlStyles.ResizeRedraw
            | ControlStyles.UserPaint,
            true);
        BackColor = theme.SidebarBackground;
    }

    /// <summary>Applies the palette; metrics re-scale from DeviceDpi at paint time.</summary>
    public void ApplyTheme(BrowserTheme theme)
    {
        this.theme = theme;
        BackColor = theme.SidebarBackground;
        Font = BrowserThemeManager.Instance.GetUiFont(DeviceDpi);
        Invalidate();
    }

    /// <summary>
    /// Replaces the rendered tab list. Favicon images are owned by their
    /// BrowserTab; the sidebar only borrows them for painting.
    /// </summary>
    public void UpdateTabs(IReadOnlyList<BrowserTabHeader> tabs, Guid? activeTabId)
    {
        this.tabs.Clear();
        this.tabs.AddRange(tabs);
        this.activeTabId = activeTabId;
        ClampScrollOffset();
        UpdateHoverState(PointToClient(MousePosition));
        Invalidate();
    }

    private int RowHeight => BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.TabRowHeight, DeviceDpi);

    private int EdgePadding => BrowserTheme.Metrics.Scale(6, DeviceDpi);

    /// <summary>Total row count: one per tab plus the trailing new-tab row.</summary>
    private int RowCount => tabs.Count + 1;

    private int ContentHeight => (EdgePadding * 2) + (RowCount * RowHeight);

    protected override void OnPaint(PaintEventArgs args)
    {
        base.OnPaint(args);
        args.Graphics.Clear(theme.SidebarBackground);

        for (var rowIndex = 0; rowIndex < RowCount; rowIndex++)
        {
            var rowBounds = GetRowBounds(rowIndex);
            if (rowBounds.Bottom < args.ClipRectangle.Top || rowBounds.Top > args.ClipRectangle.Bottom)
            {
                continue;
            }

            if (rowIndex < tabs.Count)
            {
                DrawTabRow(args.Graphics, tabs[rowIndex], rowBounds, rowIndex);
            }
            else
            {
                DrawNewTabRow(args.Graphics, rowBounds, rowIndex);
            }
        }
    }

    private void DrawTabRow(Graphics graphics, BrowserTabHeader tab, Rectangle rowBounds, int rowIndex)
    {
        var isActive = tab.Id == activeTabId;
        var isHovered = rowIndex == hoverRowIndex;
        var background = isActive
            ? theme.TabActiveBackground
            : isHovered
                ? theme.ControlHover
                : theme.SidebarBackground;
        using (var backgroundBrush = new SolidBrush(background))
        {
            graphics.FillRectangle(backgroundBrush, rowBounds);
        }

        if (isActive)
        {
            var accentWidth = BrowserTheme.Metrics.Scale(3, DeviceDpi);
            using var accentBrush = new SolidBrush(theme.Accent);
            graphics.FillRectangle(
                accentBrush,
                new Rectangle(rowBounds.Left, rowBounds.Top, accentWidth, rowBounds.Height));
        }

        var iconBounds = GetIconBounds(rowBounds);
        DrawFavicon(graphics, tab, iconBounds);

        var closeBounds = GetCloseButtonBounds(rowBounds);
        var titleRight = isHovered ? closeBounds.Left : rowBounds.Right - EdgePadding;
        var titleBounds = Rectangle.FromLTRB(
            iconBounds.Right + EdgePadding,
            rowBounds.Top,
            Math.Max(iconBounds.Right + EdgePadding, titleRight),
            rowBounds.Bottom);
        var title = tab.IsLoading
            ? BrowserStrings.Format("sidebar.loadingTitleFormat", tab.Title)
            : tab.Title;
        TextRenderer.DrawText(
            graphics,
            title,
            Font,
            titleBounds,
            isActive ? theme.TabActiveForeground : theme.BarForeground,
            TextFormatFlags.Left
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.EndEllipsis
            | TextFormatFlags.NoPrefix);

        if (isHovered)
        {
            DrawCloseButton(graphics, closeBounds, isActive);
        }
    }

    private void DrawNewTabRow(Graphics graphics, Rectangle rowBounds, int rowIndex)
    {
        if (rowIndex == hoverRowIndex)
        {
            using var hoverBrush = new SolidBrush(theme.ControlHover);
            graphics.FillRectangle(hoverBrush, rowBounds);
        }

        var iconBounds = GetIconBounds(rowBounds);
        var strokeWidth = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
        using (var plusPen = new Pen(theme.BarForegroundMuted, strokeWidth))
        {
            var centerX = iconBounds.Left + (iconBounds.Width / 2);
            var centerY = iconBounds.Top + (iconBounds.Height / 2);
            var armLength = iconBounds.Width / 2 - strokeWidth;
            graphics.DrawLine(plusPen, centerX - armLength, centerY, centerX + armLength, centerY);
            graphics.DrawLine(plusPen, centerX, centerY - armLength, centerX, centerY + armLength);
        }

        var labelBounds = Rectangle.FromLTRB(
            iconBounds.Right + EdgePadding,
            rowBounds.Top,
            rowBounds.Right - EdgePadding,
            rowBounds.Bottom);
        TextRenderer.DrawText(
            graphics,
            BrowserStrings.Get("sidebar.newTab"),
            Font,
            labelBounds,
            theme.BarForegroundMuted,
            TextFormatFlags.Left
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.EndEllipsis
            | TextFormatFlags.NoPrefix);
    }

    private void DrawFavicon(Graphics graphics, BrowserTabHeader tab, Rectangle iconBounds)
    {
        if (tab.Favicon is Image favicon)
        {
            try
            {
                graphics.DrawImage(favicon, iconBounds);
                return;
            }
            catch (ArgumentException)
            {
                // The image was disposed with its closing tab (or its data is
                // undecodable) between sync passes: fall back to the avatar.
            }
        }

        DrawLetterAvatar(graphics, tab.Title, iconBounds);
    }

    private void DrawLetterAvatar(Graphics graphics, string title, Rectangle iconBounds)
    {
        var previousSmoothing = graphics.SmoothingMode;
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        using (var avatarBrush = new SolidBrush(theme.Accent))
        {
            graphics.FillEllipse(avatarBrush, iconBounds);
        }

        graphics.SmoothingMode = previousSmoothing;

        var trimmedTitle = title.TrimStart();
        if (trimmedTitle.Length == 0)
        {
            return;
        }

        var letter = char.ToUpperInvariant(trimmedTitle[0]).ToString();
        var letterFont = BrowserThemeManager.Instance.GetFont(
            BrowserTheme.UiFontFamily,
            BrowserTheme.UiFontPointSize - 2f,
            DeviceDpi);
        // The accent (#165dff) is shared by both palettes and dark enough
        // for a white glyph in light and dark mode alike.
        TextRenderer.DrawText(
            graphics,
            letter,
            letterFont,
            iconBounds,
            Color.White,
            TextFormatFlags.HorizontalCenter
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.NoPrefix
            | TextFormatFlags.NoPadding);
    }

    private void DrawCloseButton(Graphics graphics, Rectangle closeBounds, bool isActiveRow)
    {
        if (hoverOnCloseButton)
        {
            using var hoverBrush = new SolidBrush(theme.ControlPressed);
            graphics.FillRectangle(hoverBrush, closeBounds);
        }

        var crossInset = BrowserTheme.Metrics.Scale(7, DeviceDpi);
        var crossBounds = Rectangle.Inflate(closeBounds, -crossInset, -crossInset);
        var strokeWidth = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
        var crossColor = hoverOnCloseButton
            ? theme.BarForeground
            : isActiveRow
                ? theme.TabActiveForeground
                : theme.BarForegroundMuted;
        var previousSmoothing = graphics.SmoothingMode;
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        using (var crossPen = new Pen(crossColor, strokeWidth))
        {
            graphics.DrawLine(crossPen, crossBounds.Left, crossBounds.Top, crossBounds.Right, crossBounds.Bottom);
            graphics.DrawLine(crossPen, crossBounds.Left, crossBounds.Bottom, crossBounds.Right, crossBounds.Top);
        }

        graphics.SmoothingMode = previousSmoothing;
    }

    private Rectangle GetRowBounds(int rowIndex)
    {
        return new Rectangle(
            0,
            EdgePadding + (rowIndex * RowHeight) - scrollOffset,
            ClientSize.Width,
            RowHeight);
    }

    private Rectangle GetIconBounds(Rectangle rowBounds)
    {
        var iconSize = BrowserTheme.Metrics.Scale(16, DeviceDpi);
        return new Rectangle(
            rowBounds.Left + (EdgePadding * 2),
            rowBounds.Top + ((rowBounds.Height - iconSize) / 2),
            iconSize,
            iconSize);
    }

    private Rectangle GetCloseButtonBounds(Rectangle rowBounds)
    {
        var buttonSize = BrowserTheme.Metrics.Scale(24, DeviceDpi);
        return new Rectangle(
            rowBounds.Right - EdgePadding - buttonSize,
            rowBounds.Top + ((rowBounds.Height - buttonSize) / 2),
            buttonSize,
            buttonSize);
    }

    private int GetRowIndexAt(Point location)
    {
        if (location.X < 0 || location.X >= ClientSize.Width)
        {
            return -1;
        }

        var rowIndex = (location.Y + scrollOffset - EdgePadding) / RowHeight;
        if (location.Y + scrollOffset < EdgePadding || rowIndex < 0 || rowIndex >= RowCount)
        {
            return -1;
        }

        return rowIndex;
    }

    protected override void OnMouseMove(MouseEventArgs args)
    {
        base.OnMouseMove(args);
        UpdateHoverState(args.Location);
    }

    protected override void OnMouseLeave(EventArgs args)
    {
        base.OnMouseLeave(args);
        if (hoverRowIndex != -1 || hoverOnCloseButton)
        {
            hoverRowIndex = -1;
            hoverOnCloseButton = false;
            Invalidate();
        }
    }

    protected override void OnMouseClick(MouseEventArgs args)
    {
        base.OnMouseClick(args);
        var rowIndex = GetRowIndexAt(args.Location);
        if (rowIndex < 0)
        {
            return;
        }

        if (rowIndex == tabs.Count)
        {
            if (args.Button == MouseButtons.Left)
            {
                NewTabRequested?.Invoke(this, EventArgs.Empty);
            }

            return;
        }

        var tabId = tabs[rowIndex].Id;
        if (args.Button == MouseButtons.Middle)
        {
            TabCloseRequested?.Invoke(this, new BrowserTabRequestedEventArgs(tabId));
            return;
        }

        if (args.Button != MouseButtons.Left)
        {
            return;
        }

        if (GetCloseButtonBounds(GetRowBounds(rowIndex)).Contains(args.Location))
        {
            TabCloseRequested?.Invoke(this, new BrowserTabRequestedEventArgs(tabId));
        }
        else
        {
            TabSelected?.Invoke(this, new BrowserTabRequestedEventArgs(tabId));
        }
    }

    protected override void OnMouseWheel(MouseEventArgs args)
    {
        base.OnMouseWheel(args);
        var nextOffset = Math.Clamp(
            scrollOffset - (Math.Sign(args.Delta) * RowHeight),
            0,
            Math.Max(0, ContentHeight - ClientSize.Height));
        if (nextOffset != scrollOffset)
        {
            scrollOffset = nextOffset;
            UpdateHoverState(args.Location);
            Invalidate();
        }
    }

    protected override void OnResize(EventArgs args)
    {
        base.OnResize(args);
        ClampScrollOffset();
    }

    private void ClampScrollOffset()
    {
        scrollOffset = Math.Clamp(scrollOffset, 0, Math.Max(0, ContentHeight - ClientSize.Height));
    }

    private void UpdateHoverState(Point location)
    {
        var rowIndex = ClientRectangle.Contains(location) ? GetRowIndexAt(location) : -1;
        var onCloseButton = rowIndex >= 0
            && rowIndex < tabs.Count
            && GetCloseButtonBounds(GetRowBounds(rowIndex)).Contains(location);
        if (rowIndex != hoverRowIndex || onCloseButton != hoverOnCloseButton)
        {
            hoverRowIndex = rowIndex;
            hoverOnCloseButton = onCloseButton;
            Invalidate();
        }
    }
}

/// <summary>
/// Per-tab display state handed to the sidebar. The favicon image stays
/// owned (and disposed) by the source BrowserTab.
/// </summary>
internal sealed record BrowserTabHeader(Guid Id, string Title, bool IsLoading, Image? Favicon);

internal sealed class BrowserTabRequestedEventArgs : EventArgs
{
    public BrowserTabRequestedEventArgs(Guid tabId)
    {
        TabId = tabId;
    }

    public Guid TabId { get; }
}
