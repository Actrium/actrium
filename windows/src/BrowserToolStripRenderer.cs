using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace Actrium.Windows;

/// <summary>
/// Theme-driven ToolStrip/menu renderer. The color table alone only covers
/// backgrounds and selection fills, so item text, check marks, submenu
/// arrows and separators are rendered here explicitly — otherwise dark mode
/// keeps black system glyphs on dark surfaces.
/// </summary>
internal sealed class BrowserToolStripRenderer : ToolStripProfessionalRenderer
{
    private readonly BrowserTheme theme;

    public BrowserToolStripRenderer(BrowserTheme theme)
        : base(new BrowserToolStripColorTable(theme))
    {
        this.theme = theme;
        RoundedEdges = false;
    }

    protected override void OnRenderItemText(ToolStripItemTextRenderEventArgs e)
    {
        e.TextColor = e.Item.Enabled ? theme.BarForeground : theme.DisabledForeground;
        base.OnRenderItemText(e);
    }

    protected override void OnRenderItemCheck(ToolStripItemImageRenderEventArgs e)
    {
        var bounds = e.ImageRectangle;
        var color = e.Item.Enabled ? theme.BarForeground : theme.DisabledForeground;
        using var pen = new Pen(color, 2f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
        };
        var previousSmoothing = e.Graphics.SmoothingMode;
        e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
        e.Graphics.DrawLines(pen, new[]
        {
            new PointF(bounds.Left + (bounds.Width * 0.25f), bounds.Top + (bounds.Height * 0.55f)),
            new PointF(bounds.Left + (bounds.Width * 0.45f), bounds.Top + (bounds.Height * 0.75f)),
            new PointF(bounds.Left + (bounds.Width * 0.78f), bounds.Top + (bounds.Height * 0.30f)),
        });
        e.Graphics.SmoothingMode = previousSmoothing;
    }

    protected override void OnRenderArrow(ToolStripArrowRenderEventArgs e)
    {
        e.ArrowColor = e.Item?.Enabled != false ? theme.BarForeground : theme.DisabledForeground;
        base.OnRenderArrow(e);
    }

    protected override void OnRenderSeparator(ToolStripSeparatorRenderEventArgs e)
    {
        var bounds = e.Item.ContentRectangle;
        using var pen = new Pen(theme.Border);
        if (e.Vertical)
        {
            var x = bounds.Left + (bounds.Width / 2);
            e.Graphics.DrawLine(pen, x, bounds.Top + 2, x, bounds.Bottom - 2);
        }
        else
        {
            var y = bounds.Top + (bounds.Height / 2);
            e.Graphics.DrawLine(pen, bounds.Left + 2, y, bounds.Right - 2, y);
        }
    }
}

/// <summary>Palette-built color table backing <see cref="BrowserToolStripRenderer"/>.</summary>
internal sealed class BrowserToolStripColorTable : ProfessionalColorTable
{
    private readonly BrowserTheme theme;

    public BrowserToolStripColorTable(BrowserTheme theme)
    {
        this.theme = theme;
        UseSystemColors = false;
    }

    public override Color MenuStripGradientBegin => theme.BarBackground;

    public override Color MenuStripGradientEnd => theme.BarBackground;

    public override Color ToolStripDropDownBackground => theme.CardBackground;

    public override Color ImageMarginGradientBegin => theme.CardBackground;

    public override Color ImageMarginGradientMiddle => theme.CardBackground;

    public override Color ImageMarginGradientEnd => theme.CardBackground;

    public override Color MenuItemSelected => theme.ControlHover;

    public override Color MenuItemSelectedGradientBegin => theme.ControlHover;

    public override Color MenuItemSelectedGradientEnd => theme.ControlHover;

    public override Color MenuItemPressedGradientBegin => theme.ControlPressed;

    public override Color MenuItemPressedGradientMiddle => theme.ControlPressed;

    public override Color MenuItemPressedGradientEnd => theme.ControlPressed;

    public override Color MenuItemBorder => theme.ControlHover;

    public override Color MenuBorder => theme.Border;

    public override Color SeparatorDark => theme.Border;

    public override Color SeparatorLight => theme.Border;

    public override Color ToolStripBorder => theme.Border;

    public override Color ToolStripGradientBegin => theme.BarBackground;

    public override Color ToolStripGradientMiddle => theme.BarBackground;

    public override Color ToolStripGradientEnd => theme.BarBackground;

    public override Color ButtonSelectedHighlight => theme.ControlHover;

    public override Color ButtonSelectedGradientBegin => theme.ControlHover;

    public override Color ButtonSelectedGradientEnd => theme.ControlHover;

    public override Color ButtonPressedHighlight => theme.ControlPressed;

    public override Color ButtonPressedGradientBegin => theme.ControlPressed;

    public override Color ButtonPressedGradientEnd => theme.ControlPressed;

    public override Color CheckBackground => theme.CardBackground;

    public override Color CheckSelectedBackground => theme.ControlPressed;

    public override Color CheckPressedBackground => theme.ControlPressed;
}
