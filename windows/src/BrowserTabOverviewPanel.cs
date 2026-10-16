using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace Actrium.Windows;

/// <summary>
/// Tab overview surface (macOS Overview canvas): an owner-drawn card grid
/// overlaying the content host, one card per tab — the snapshot cached on
/// tab deactivation, the title and a hover close button — plus a trailing
/// "+" card. A tab that never produced a snapshot renders a themed
/// letter-avatar placeholder instead. Left click focuses the tab, the close
/// button or a middle click closes it, and the "+" card opens a new tab.
/// Grid geometry comes from BrowserTabOverviewPolicy; MainForm owns
/// visibility, the z-order above the WebView2 child HWND and the
/// Ctrl+Shift+A / Escape wiring.
/// </summary>
internal sealed class BrowserTabOverviewPanel : UserControl
{
    private readonly List<BrowserTabOverviewCard> cards = [];
    private IReadOnlyList<Rectangle> cardRects = [];
    private BrowserTheme theme = BrowserTheme.Light;
    private int scrollOffset;
    private int hoverCellIndex = -1;
    private bool hoverOnCloseButton;

    public event EventHandler? NewTabRequested;
    public event EventHandler<BrowserTabRequestedEventArgs>? TabSelected;
    public event EventHandler<BrowserTabRequestedEventArgs>? TabCloseRequested;

    public BrowserTabOverviewPanel()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        Dock = DockStyle.Fill;
        Visible = false;
        SetStyle(
            ControlStyles.AllPaintingInWmPaint
            | ControlStyles.OptimizedDoubleBuffer
            | ControlStyles.ResizeRedraw
            | ControlStyles.UserPaint,
            true);
        BackColor = theme.WindowBackground;
    }

    /// <summary>Applies the palette; metrics re-scale from DeviceDpi at layout time.</summary>
    public void ApplyTheme(BrowserTheme theme)
    {
        this.theme = theme;
        BackColor = theme.WindowBackground;
        Font = BrowserThemeManager.Instance.GetUiFont(DeviceDpi);
        RecomputeLayout();
        Invalidate();
    }

    /// <summary>
    /// Replaces the rendered card list. Snapshot images stay owned (and
    /// disposed) by their BrowserTab; the panel only borrows them for
    /// painting, mirroring the sidebar's favicon ownership rule.
    /// </summary>
    public void UpdateCards(IReadOnlyList<BrowserTabOverviewCard> cards)
    {
        this.cards.Clear();
        this.cards.AddRange(cards);
        RecomputeLayout();
        UpdateHoverState(PointToClient(MousePosition));
        Invalidate();
    }

    /// <summary>Total cell count: one card per tab plus the trailing "+" card.</summary>
    private int CellCount => cards.Count + 1;

    private int ContentHeight =>
        BrowserTabOverviewPolicy.ContentHeight(ClientSize.Width, CellCount, DeviceDpi);

    protected override void OnPaint(PaintEventArgs args)
    {
        base.OnPaint(args);
        args.Graphics.Clear(theme.WindowBackground);

        for (var cellIndex = 0; cellIndex < cardRects.Count; cellIndex++)
        {
            var cellBounds = GetCellBounds(cellIndex);
            if (cellBounds.Bottom < args.ClipRectangle.Top || cellBounds.Top > args.ClipRectangle.Bottom)
            {
                continue;
            }

            if (cellIndex < cards.Count)
            {
                DrawTabCard(args.Graphics, cards[cellIndex], cellBounds, cellIndex);
            }
            else
            {
                DrawNewTabCard(args.Graphics, cellBounds, cellIndex);
            }
        }
    }

    private void DrawTabCard(Graphics graphics, BrowserTabOverviewCard card, Rectangle cellBounds, int cellIndex)
    {
        var titleHeight = BrowserTabOverviewPolicy.CardTitleHeight(DeviceDpi);
        var thumbnailBounds = new Rectangle(
            cellBounds.Left,
            cellBounds.Top,
            cellBounds.Width,
            Math.Max(0, cellBounds.Height - titleHeight));
        var titleBounds = new Rectangle(
            cellBounds.Left,
            thumbnailBounds.Bottom,
            cellBounds.Width,
            titleHeight);

        using (var cardBrush = new SolidBrush(theme.CardBackground))
        {
            graphics.FillRectangle(cardBrush, cellBounds);
        }

        DrawSnapshot(graphics, card, thumbnailBounds);
        DrawCardBorder(graphics, cellBounds, card.IsActive, cellIndex);

        var edgePadding = BrowserTheme.Metrics.Scale(8, DeviceDpi);
        var closeBounds = GetCloseButtonBounds(cellBounds);
        var isHovered = cellIndex == hoverCellIndex;
        var titleRight = isHovered ? closeBounds.Left : titleBounds.Right - edgePadding;
        TextRenderer.DrawText(
            graphics,
            card.Title,
            Font,
            Rectangle.FromLTRB(
                titleBounds.Left + edgePadding,
                titleBounds.Top,
                Math.Max(titleBounds.Left + edgePadding, titleRight),
                titleBounds.Bottom),
            theme.BarForeground,
            TextFormatFlags.Left
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.EndEllipsis
            | TextFormatFlags.NoPrefix);

        if (isHovered)
        {
            DrawCloseButton(graphics, closeBounds);
        }
    }

    /// <summary>
    /// Paints the cached snapshot cover-cropped into the thumbnail area; a
    /// tab without one (never activated away from, capture failed from the
    /// start, or disposed mid-sync) gets the themed letter avatar.
    /// </summary>
    private void DrawSnapshot(Graphics graphics, BrowserTabOverviewCard card, Rectangle thumbnailBounds)
    {
        if (thumbnailBounds.Width <= 0 || thumbnailBounds.Height <= 0)
        {
            return;
        }

        if (card.Snapshot is Image snapshot)
        {
            try
            {
                graphics.DrawImage(
                    snapshot,
                    thumbnailBounds,
                    GetCoverSourceBounds(snapshot.Size, thumbnailBounds.Size),
                    GraphicsUnit.Pixel);
                return;
            }
            catch (ArgumentException)
            {
                // The bitmap was disposed with its closing tab between sync
                // passes: fall through to the placeholder.
            }
        }

        DrawPlaceholder(graphics, card.Title, thumbnailBounds);
    }

    /// <summary>
    /// Source crop that fills the destination while preserving the source
    /// aspect ratio (center-cropped, CSS object-fit cover behavior).
    /// </summary>
    private static Rectangle GetCoverSourceBounds(Size sourceSize, Size destinationSize)
    {
        if (sourceSize.Width <= 0
            || sourceSize.Height <= 0
            || destinationSize.Width <= 0
            || destinationSize.Height <= 0)
        {
            return new Rectangle(Point.Empty, sourceSize);
        }

        var sourceAspect = (double)sourceSize.Width / sourceSize.Height;
        var destinationAspect = (double)destinationSize.Width / destinationSize.Height;
        if (sourceAspect > destinationAspect)
        {
            var cropWidth = Math.Max(1, (int)Math.Round(sourceSize.Height * destinationAspect));
            return new Rectangle((sourceSize.Width - cropWidth) / 2, 0, cropWidth, sourceSize.Height);
        }

        var cropHeight = Math.Max(1, (int)Math.Round(sourceSize.Width / destinationAspect));
        return new Rectangle(0, (sourceSize.Height - cropHeight) / 2, sourceSize.Width, cropHeight);
    }

    private void DrawPlaceholder(Graphics graphics, string title, Rectangle thumbnailBounds)
    {
        using (var placeholderBrush = new SolidBrush(theme.SidebarBackground))
        {
            graphics.FillRectangle(placeholderBrush, thumbnailBounds);
        }

        var avatarSize = BrowserTheme.Metrics.Scale(40, DeviceDpi);
        if (thumbnailBounds.Width < avatarSize || thumbnailBounds.Height < avatarSize)
        {
            return;
        }

        var avatarBounds = new Rectangle(
            thumbnailBounds.Left + ((thumbnailBounds.Width - avatarSize) / 2),
            thumbnailBounds.Top + ((thumbnailBounds.Height - avatarSize) / 2),
            avatarSize,
            avatarSize);
        var previousSmoothing = graphics.SmoothingMode;
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        using (var avatarBrush = new SolidBrush(theme.Accent))
        {
            graphics.FillEllipse(avatarBrush, avatarBounds);
        }

        graphics.SmoothingMode = previousSmoothing;

        var trimmedTitle = title.TrimStart();
        if (trimmedTitle.Length == 0)
        {
            return;
        }

        // The accent (#165dff) is shared by both palettes and dark enough
        // for a white glyph in light and dark mode alike.
        TextRenderer.DrawText(
            graphics,
            char.ToUpperInvariant(trimmedTitle[0]).ToString(),
            BrowserThemeManager.Instance.GetFont(
                BrowserTheme.UiFontFamily,
                BrowserTheme.UiFontPointSize + 2f,
                DeviceDpi),
            avatarBounds,
            Color.White,
            TextFormatFlags.HorizontalCenter
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.NoPrefix
            | TextFormatFlags.NoPadding);
    }

    private void DrawNewTabCard(Graphics graphics, Rectangle cellBounds, int cellIndex)
    {
        var background = cellIndex == hoverCellIndex ? theme.ControlHover : theme.CardBackground;
        using (var cardBrush = new SolidBrush(background))
        {
            graphics.FillRectangle(cardBrush, cellBounds);
        }

        DrawCardBorder(graphics, cellBounds, isActive: false, cellIndex);

        var plusSize = BrowserTheme.Metrics.Scale(20, DeviceDpi);
        var labelHeight = BrowserTabOverviewPolicy.CardTitleHeight(DeviceDpi);
        var centerX = cellBounds.Left + (cellBounds.Width / 2);
        var centerY = cellBounds.Top + ((cellBounds.Height - labelHeight) / 2);
        var strokeWidth = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
        using (var plusPen = new Pen(theme.BarForegroundMuted, strokeWidth))
        {
            var armLength = plusSize / 2;
            graphics.DrawLine(plusPen, centerX - armLength, centerY, centerX + armLength, centerY);
            graphics.DrawLine(plusPen, centerX, centerY - armLength, centerX, centerY + armLength);
        }

        TextRenderer.DrawText(
            graphics,
            BrowserStrings.Get("overview.newTab"),
            Font,
            new Rectangle(
                cellBounds.Left,
                cellBounds.Bottom - labelHeight,
                cellBounds.Width,
                labelHeight),
            theme.BarForegroundMuted,
            TextFormatFlags.HorizontalCenter
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.EndEllipsis
            | TextFormatFlags.NoPrefix);
    }

    private void DrawCardBorder(Graphics graphics, Rectangle cellBounds, bool isActive, int cellIndex)
    {
        var baseThickness = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
        // The active card carries a doubled accent edge (the sidebar's
        // active-row accent translated to a card outline).
        var thickness = isActive ? baseThickness * 2 : baseThickness;
        var borderColor = isActive
            ? theme.Accent
            : cellIndex == hoverCellIndex
                ? theme.BarForegroundMuted
                : theme.Border;
        using var borderPen = new Pen(borderColor, thickness);
        var inset = thickness / 2f;
        graphics.DrawRectangle(
            borderPen,
            cellBounds.Left + inset,
            cellBounds.Top + inset,
            cellBounds.Width - thickness,
            cellBounds.Height - thickness);
    }

    private void DrawCloseButton(Graphics graphics, Rectangle closeBounds)
    {
        if (hoverOnCloseButton)
        {
            using var hoverBrush = new SolidBrush(theme.ControlPressed);
            graphics.FillRectangle(hoverBrush, closeBounds);
        }

        var crossInset = BrowserTheme.Metrics.Scale(7, DeviceDpi);
        var crossBounds = Rectangle.Inflate(closeBounds, -crossInset, -crossInset);
        var strokeWidth = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
        var crossColor = hoverOnCloseButton ? theme.BarForeground : theme.BarForegroundMuted;
        var previousSmoothing = graphics.SmoothingMode;
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        using (var crossPen = new Pen(crossColor, strokeWidth))
        {
            graphics.DrawLine(crossPen, crossBounds.Left, crossBounds.Top, crossBounds.Right, crossBounds.Bottom);
            graphics.DrawLine(crossPen, crossBounds.Left, crossBounds.Bottom, crossBounds.Right, crossBounds.Top);
        }

        graphics.SmoothingMode = previousSmoothing;
    }

    /// <summary>Cell rectangle in client coordinates (grid rect minus scroll).</summary>
    private Rectangle GetCellBounds(int cellIndex)
    {
        var gridBounds = cardRects[cellIndex];
        return gridBounds with { Y = gridBounds.Y - scrollOffset };
    }

    /// <summary>Close button square in the card's title strip, right-aligned.</summary>
    private Rectangle GetCloseButtonBounds(Rectangle cellBounds)
    {
        var buttonSize = BrowserTheme.Metrics.Scale(24, DeviceDpi);
        var titleHeight = BrowserTabOverviewPolicy.CardTitleHeight(DeviceDpi);
        var edgePadding = BrowserTheme.Metrics.Scale(8, DeviceDpi);
        return new Rectangle(
            cellBounds.Right - edgePadding - buttonSize,
            cellBounds.Bottom - titleHeight + ((titleHeight - buttonSize) / 2),
            buttonSize,
            buttonSize);
    }

    private int GetCellIndexAt(Point location)
    {
        for (var cellIndex = 0; cellIndex < cardRects.Count; cellIndex++)
        {
            if (GetCellBounds(cellIndex).Contains(location))
            {
                return cellIndex;
            }
        }

        return -1;
    }

    protected override void OnMouseMove(MouseEventArgs args)
    {
        base.OnMouseMove(args);
        UpdateHoverState(args.Location);
    }

    protected override void OnMouseLeave(EventArgs args)
    {
        base.OnMouseLeave(args);
        if (hoverCellIndex != -1 || hoverOnCloseButton)
        {
            hoverCellIndex = -1;
            hoverOnCloseButton = false;
            Invalidate();
        }
    }

    protected override void OnMouseClick(MouseEventArgs args)
    {
        base.OnMouseClick(args);
        var cellIndex = GetCellIndexAt(args.Location);
        if (cellIndex < 0)
        {
            return;
        }

        if (cellIndex == cards.Count)
        {
            if (args.Button == MouseButtons.Left)
            {
                NewTabRequested?.Invoke(this, EventArgs.Empty);
            }

            return;
        }

        var tabId = cards[cellIndex].Id;
        if (args.Button == MouseButtons.Middle)
        {
            TabCloseRequested?.Invoke(this, new BrowserTabRequestedEventArgs(tabId));
            return;
        }

        if (args.Button != MouseButtons.Left)
        {
            return;
        }

        if (GetCloseButtonBounds(GetCellBounds(cellIndex)).Contains(args.Location))
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
        var scrollStep = BrowserTheme.Metrics.Scale(48, DeviceDpi);
        var nextOffset = Math.Clamp(
            scrollOffset - (Math.Sign(args.Delta) * scrollStep),
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
        RecomputeLayout();
    }

    private void RecomputeLayout()
    {
        cardRects = BrowserTabOverviewPolicy.CardRects(ClientSize.Width, CellCount, DeviceDpi);
        scrollOffset = Math.Clamp(scrollOffset, 0, Math.Max(0, ContentHeight - ClientSize.Height));
    }

    private void UpdateHoverState(Point location)
    {
        var cellIndex = ClientRectangle.Contains(location) ? GetCellIndexAt(location) : -1;
        var onCloseButton = cellIndex >= 0
            && cellIndex < cards.Count
            && GetCloseButtonBounds(GetCellBounds(cellIndex)).Contains(location);
        if (cellIndex != hoverCellIndex || onCloseButton != hoverOnCloseButton)
        {
            hoverCellIndex = cellIndex;
            hoverOnCloseButton = onCloseButton;
            Invalidate();
        }
    }
}

/// <summary>
/// Per-tab display state handed to the overview grid. The snapshot image
/// stays owned (and disposed) by the source BrowserTab.
/// </summary>
internal sealed record BrowserTabOverviewCard(Guid Id, string Title, Image? Snapshot, bool IsActive);
