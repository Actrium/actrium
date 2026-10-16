using System.Drawing;

namespace Actrium.Windows;

/// <summary>
/// Pure grid geometry for the tab overview surface: how many card columns
/// fit a viewport, each card's rectangle (16:10 thumbnail over a
/// fixed-height title strip) and the total scrollable grid height. Cells
/// flow left to right, top to bottom in tab order, with the trailing "+"
/// card as the last cell. Card widths clamp between the scaled minimum and
/// maximum, and the grid centers horizontally once the maximum caps the
/// cards. All inputs and outputs are pixels at the caller's DPI; the 96-DPI
/// base values resolve through <see cref="BrowserTheme.Metrics.Scale"/> so
/// the overview shares the single chrome metrics source.
/// Harness-linked: this file must stay WinForms-free.
/// </summary>
internal static class BrowserTabOverviewPolicy
{
    /// <summary>Outer grid margin at 96 DPI.</summary>
    public const int OuterPaddingBase = 24;

    /// <summary>Gap between adjacent cards at 96 DPI.</summary>
    public const int CardGapBase = 16;

    /// <summary>Narrowest useful card at 96 DPI; fewer columns win below it.</summary>
    public const int CardMinWidthBase = 200;

    /// <summary>Card width cap at 96 DPI; wider viewports center the grid.</summary>
    public const int CardMaxWidthBase = 320;

    /// <summary>Title strip height under the thumbnail at 96 DPI.</summary>
    public const int CardTitleHeightBase = 36;

    /// <summary>Thumbnail aspect ratio (16:10, close to a typical page viewport).</summary>
    public const int ThumbnailAspectWidth = 16;

    public const int ThumbnailAspectHeight = 10;

    public static int OuterPadding(int dpi)
    {
        return BrowserTheme.Metrics.Scale(OuterPaddingBase, dpi);
    }

    public static int CardGap(int dpi)
    {
        return BrowserTheme.Metrics.Scale(CardGapBase, dpi);
    }

    public static int CardMinWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(CardMinWidthBase, dpi);
    }

    public static int CardMaxWidth(int dpi)
    {
        return BrowserTheme.Metrics.Scale(CardMaxWidthBase, dpi);
    }

    public static int CardTitleHeight(int dpi)
    {
        return BrowserTheme.Metrics.Scale(CardTitleHeightBase, dpi);
    }

    /// <summary>
    /// Columns for a viewport: as many minimum-width cards as fit the
    /// padded content band, at least one, never more than the cell count.
    /// </summary>
    public static int Columns(int viewportWidth, int cellCount, int dpi)
    {
        if (cellCount <= 0)
        {
            return 0;
        }

        var contentWidth = viewportWidth - (2 * OuterPadding(dpi));
        var stride = CardMinWidth(dpi) + CardGap(dpi);
        var fittingColumns = (contentWidth + CardGap(dpi)) / stride;
        return Math.Clamp(fittingColumns, 1, cellCount);
    }

    /// <summary>
    /// Card width for a column count: the padded content width split across
    /// the columns, capped at the scaled maximum and floored at one pixel so
    /// transient mid-layout viewports cannot produce degenerate rectangles.
    /// </summary>
    public static int CardWidth(int viewportWidth, int columns, int dpi)
    {
        if (columns <= 0)
        {
            return 0;
        }

        var contentWidth = viewportWidth - (2 * OuterPadding(dpi));
        var width = (contentWidth - ((columns - 1) * CardGap(dpi))) / columns;
        return Math.Clamp(width, 1, CardMaxWidth(dpi));
    }

    /// <summary>Snapshot area height for a card width under the 16:10 aspect.</summary>
    public static int ThumbnailHeight(int cardWidth)
    {
        return cardWidth * ThumbnailAspectHeight / ThumbnailAspectWidth;
    }

    /// <summary>Full card height: thumbnail plus the scaled title strip.</summary>
    public static int CardHeight(int cardWidth, int dpi)
    {
        return ThumbnailHeight(cardWidth) + CardTitleHeight(dpi);
    }

    /// <summary>
    /// One rectangle per cell (tabs in order, then the "+" card), flowing
    /// left to right, top to bottom from the padded top-left corner. The
    /// grid centers horizontally when the card width cap leaves space over.
    /// </summary>
    public static IReadOnlyList<Rectangle> CardRects(int viewportWidth, int cellCount, int dpi)
    {
        if (cellCount <= 0)
        {
            return [];
        }

        var columns = Columns(viewportWidth, cellCount, dpi);
        var cardWidth = CardWidth(viewportWidth, columns, dpi);
        var cardHeight = CardHeight(cardWidth, dpi);
        var gap = CardGap(dpi);
        var padding = OuterPadding(dpi);
        var gridWidth = (columns * cardWidth) + ((columns - 1) * gap);
        var originX = padding + Math.Max(0, (viewportWidth - (2 * padding) - gridWidth) / 2);

        var cardRects = new Rectangle[cellCount];
        for (var cellIndex = 0; cellIndex < cellCount; cellIndex++)
        {
            var column = cellIndex % columns;
            var row = cellIndex / columns;
            cardRects[cellIndex] = new Rectangle(
                originX + (column * (cardWidth + gap)),
                padding + (row * (cardHeight + gap)),
                cardWidth,
                cardHeight);
        }

        return cardRects;
    }

    /// <summary>Scrollable grid height: card rows plus the outer padding.</summary>
    public static int ContentHeight(int viewportWidth, int cellCount, int dpi)
    {
        if (cellCount <= 0)
        {
            return 0;
        }

        var columns = Columns(viewportWidth, cellCount, dpi);
        var rows = (cellCount + columns - 1) / columns;
        var cardHeight = CardHeight(CardWidth(viewportWidth, columns, dpi), dpi);
        return (2 * OuterPadding(dpi)) + (rows * cardHeight) + ((rows - 1) * CardGap(dpi));
    }
}
