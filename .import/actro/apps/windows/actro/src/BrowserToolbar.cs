using System.Drawing;
using System.Drawing.Text;
using System.Windows.Forms;

namespace Actro.Windows;

/// <summary>
/// Two-segment toolbar mirroring the macOS structure with Chrome-ordered
/// details. The sidebar segment (above the vertical tab sidebar) right-aligns
/// [+ new tab] next to the reserved tab-overview slot and shrinks to a fixed
/// stub while the sidebar is collapsed; the content segment orders
/// [sidebar toggle][back][forward][reload-stop][address box][ellipsis menu]
/// like Chrome/Edge, with the bookmark star inside the address box's right
/// edge. Geometry comes from BrowserToolbarLayoutPolicy, glyphs from
/// BrowserGlyphs; there is no Home button (home stays a menu command plus
/// Alt+Home) and no status column (statuses live in the MainForm overlay).
/// </summary>
internal sealed class BrowserToolbar : UserControl
{
    private readonly BrowserThemedButton newTabButton;
    private readonly BrowserThemedButton sidebarToggleButton;
    private readonly BrowserThemedButton backButton;
    private readonly BrowserThemedButton forwardButton;
    private readonly BrowserThemedButton reloadStopButton;
    private readonly BrowserThemedButton bookmarkButton;
    private readonly BrowserThemedButton menuButton;
    private readonly BrowserThemedBorderPanel addressPanel;
    private readonly TextBox addressBox;
    private readonly ToolTip toolTip;
    private BrowserTheme theme = BrowserTheme.Light;
    private int sidebarWidth;
    private bool sidebarCollapsed;
    private bool isBookmarked;

    public event EventHandler? NewTabRequested;
    public event EventHandler? SidebarToggleRequested;
    public event EventHandler? BackRequested;
    public event EventHandler? ForwardRequested;
    // One button serves reload and stop (Chrome behavior): the shell's
    // routed handler resolves which command applies to the loading state,
    // so the toolbar keeps a single contract event for both.
    public event EventHandler? ReloadRequested;
    public event EventHandler? BookmarkRequested;
    public event EventHandler? MenuRequested;
    public event EventHandler<BrowserAddressRequestedEventArgs>? AddressRequested;

    public BrowserToolbar()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        Dock = DockStyle.Top;

        toolTip = new ToolTip();
        newTabButton = CreateGlyphButton(BrowserGlyphs.Get(BrowserMenuCommand.NewTab), "toolbar.tooltip.newTab");
        sidebarToggleButton = CreateGlyphButton(BrowserGlyphs.Get(BrowserMenuCommand.ToggleSidebar), "toolbar.tooltip.toggleSidebar");
        backButton = CreateGlyphButton(BrowserGlyphs.Get(BrowserMenuCommand.Back), "toolbar.tooltip.back");
        forwardButton = CreateGlyphButton(BrowserGlyphs.Get(BrowserMenuCommand.Forward), "toolbar.tooltip.forward");
        reloadStopButton = CreateGlyphButton(BrowserGlyphs.Get(BrowserMenuCommand.Reload), "toolbar.tooltip.reload");
        bookmarkButton = CreateGlyphButton(BrowserGlyphs.Get(BrowserMenuCommand.BookmarkPage), "toolbar.tooltip.bookmark");
        menuButton = CreateGlyphButton(BrowserGlyphs.AppMenu, "toolbar.tooltip.menu");

        addressBox = new TextBox
        {
            BorderStyle = BorderStyle.None,
        };
        // The bookmark star lives inside the address box's right edge
        // (Chrome placement), so it rides on the input surface, not the bar.
        addressPanel = new BrowserThemedBorderPanel(addressBox, bookmarkButton);

        Controls.Add(newTabButton);
        Controls.Add(sidebarToggleButton);
        Controls.Add(backButton);
        Controls.Add(forwardButton);
        Controls.Add(reloadStopButton);
        Controls.Add(addressPanel);
        Controls.Add(menuButton);

        newTabButton.Click += (_, _) => NewTabRequested?.Invoke(this, EventArgs.Empty);
        sidebarToggleButton.Click += (_, _) => SidebarToggleRequested?.Invoke(this, EventArgs.Empty);
        backButton.Click += (_, _) => BackRequested?.Invoke(this, EventArgs.Empty);
        forwardButton.Click += (_, _) => ForwardRequested?.Invoke(this, EventArgs.Empty);
        reloadStopButton.Click += (_, _) => ReloadRequested?.Invoke(this, EventArgs.Empty);
        bookmarkButton.Click += (_, _) => BookmarkRequested?.Invoke(this, EventArgs.Empty);
        menuButton.Click += (_, _) => MenuRequested?.Invoke(this, EventArgs.Empty);
        addressBox.KeyDown += OnAddressKeyDown;

        sidebarWidth = BrowserSidebarLayoutPolicy.DefaultWidth(DeviceDpi);
        ApplyTheme(BrowserTheme.Light);
        SetNavigationState(canGoBack: false, canGoForward: false, isLoading: false);
        SetBookmarkState(canBookmark: false, isBookmarked: false);
    }

    /// <summary>Applies palette, typography and DPI-scaled metrics in one pass.</summary>
    public void ApplyTheme(BrowserTheme theme)
    {
        this.theme = theme;
        var dpi = DeviceDpi;
        var barHeight = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.ToolbarHeight, dpi);
        Height = barHeight;
        MinimumSize = new Size(0, barHeight);
        BackColor = theme.BarBackground;

        var glyphFont = GetGlyphFont(dpi);
        foreach (var button in new[]
        {
            newTabButton, sidebarToggleButton, backButton, forwardButton,
            reloadStopButton, bookmarkButton, menuButton,
        })
        {
            button.Font = glyphFont;
            button.ApplyTheme(theme);
        }

        // The star sits on the input surface: resting on InputBackground,
        // filled with the shared accent while the page is bookmarked.
        bookmarkButton.RestingBackground = theme.InputBackground;
        UpdateBookmarkButton();

        addressBox.Font = BrowserThemeManager.Instance.GetUiFont(dpi);
        addressPanel.ApplyTheme(theme);
        PerformSegmentLayout();
    }

    /// <summary>
    /// Aligns the sidebar segment with the live sidebar geometry; the shell
    /// calls this whenever the splitter moves, the sidebar collapses or
    /// expands, or the DPI changes.
    /// </summary>
    public void SetSidebarLayout(int sidebarWidthPixels, bool collapsed)
    {
        sidebarWidth = sidebarWidthPixels;
        sidebarCollapsed = collapsed;
        PerformSegmentLayout();
    }

    public void SetAddress(string address)
    {
        if (!addressBox.Focused)
        {
            addressBox.Text = address;
        }
    }

    /// <summary>
    /// Loading indication is the reload-to-stop button flip (glyph and
    /// tooltip); the sidebar row marker carries per-tab state. There is no
    /// "Loading" status string anymore.
    /// </summary>
    public void SetNavigationState(bool canGoBack, bool canGoForward, bool isLoading)
    {
        backButton.Enabled = canGoBack;
        forwardButton.Enabled = canGoForward;
        var command = isLoading ? BrowserMenuCommand.Stop : BrowserMenuCommand.Reload;
        reloadStopButton.Text = BrowserGlyphs.Get(command).ToString();
        toolTip.SetToolTip(
            reloadStopButton,
            BrowserStrings.Get(isLoading ? "toolbar.tooltip.stop" : "toolbar.tooltip.reload"));
    }

    /// <summary>Star state: outline to add, accent-filled to remove.</summary>
    public void SetBookmarkState(bool canBookmark, bool isBookmarked)
    {
        bookmarkButton.Enabled = canBookmark;
        this.isBookmarked = isBookmarked;
        UpdateBookmarkButton();
    }

    public void FocusAddress()
    {
        addressBox.Focus();
        addressBox.SelectAll();
    }

    /// <summary>
    /// Drops the compact app menu below its ellipsis button, right-aligned
    /// to the button edge like the Chrome/Edge three-dot menu.
    /// </summary>
    public void ShowAppMenu(ContextMenuStrip menu)
    {
        menu.Show(
            menuButton,
            new Point(menuButton.Width, menuButton.Height),
            ToolStripDropDownDirection.BelowLeft);
    }

    protected override void OnResize(EventArgs args)
    {
        base.OnResize(args);
        PerformSegmentLayout();
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            toolTip.Dispose();
        }

        base.Dispose(disposing);
    }

    /// <summary>
    /// Positions both segments from the pure layout policy: sidebar segment
    /// buttons right-aligned against the reserved overview slot, content
    /// segment Chrome-ordered with the address box centered in the leftover
    /// space up to its width cap.
    /// </summary>
    private void PerformSegmentLayout()
    {
        var dpi = DeviceDpi;
        var segments = BrowserToolbarLayoutPolicy.Segments(
            sidebarWidth,
            sidebarCollapsed,
            ClientSize.Width,
            dpi);
        var buttonWidth = BrowserToolbarLayoutPolicy.ButtonWidth(dpi);
        var buttonGap = BrowserToolbarLayoutPolicy.ButtonGap(dpi);
        var edgePadding = BrowserToolbarLayoutPolicy.EdgePadding(dpi);
        var buttonHeight = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.InputHeight, dpi);
        var top = Math.Max(0, (ClientSize.Height - buttonHeight) / 2);

        // Sidebar segment: [+ new tab][overview slot], right-aligned.
        var overviewSlotLeft = segments.SidebarSegmentWidth
            - edgePadding
            - BrowserToolbarLayoutPolicy.OverviewSlotWidth(dpi);
        newTabButton.SetBounds(
            overviewSlotLeft - buttonGap - buttonWidth,
            top,
            buttonWidth,
            buttonHeight);

        // Content segment: nav cluster, then the address box, then the menu.
        var x = segments.ContentSegmentLeft + edgePadding;
        foreach (var button in new[] { sidebarToggleButton, backButton, forwardButton, reloadStopButton })
        {
            button.SetBounds(x, top, buttonWidth, buttonHeight);
            x += buttonWidth + buttonGap;
        }

        var menuLeft = ClientSize.Width - edgePadding - buttonWidth;
        menuButton.SetBounds(menuLeft, top, buttonWidth, buttonHeight);

        var addressAreaLeft = x + buttonGap;
        var availableWidth = Math.Max(0, menuLeft - buttonGap - addressAreaLeft);
        var addressWidth = BrowserToolbarLayoutPolicy.AddressWidth(availableWidth, dpi);
        addressPanel.SetBounds(
            addressAreaLeft + ((availableWidth - addressWidth) / 2),
            top,
            addressWidth,
            buttonHeight);
    }

    private void UpdateBookmarkButton()
    {
        bookmarkButton.Text = (isBookmarked
            ? BrowserGlyphs.BookmarkStarFilled
            : BrowserGlyphs.BookmarkStar).ToString();
        bookmarkButton.EnabledForeground = isBookmarked ? theme.Accent : null;
        toolTip.SetToolTip(
            bookmarkButton,
            BrowserStrings.Get(isBookmarked ? "toolbar.tooltip.removeBookmark" : "toolbar.tooltip.bookmark"));
    }

    private BrowserThemedButton CreateGlyphButton(char glyph, string tooltipKey)
    {
        var button = new BrowserThemedButton
        {
            Text = glyph.ToString(),
        };
        toolTip.SetToolTip(button, BrowserStrings.Get(tooltipKey));
        return button;
    }

    private static Font GetGlyphFont(int dpi)
    {
        return BrowserThemeManager.Instance.GetFont(
            GlyphFontFamily.Value,
            BrowserGlyphs.GlyphPointSize,
            dpi);
    }

    /// <summary>
    /// First installed family from the theme's glyph stack. On Win10 19041
    /// Segoe Fluent Icons is absent, so this lands on Segoe MDL2 Assets —
    /// which is why BrowserGlyphs restricts itself to MDL2 codepoints. A
    /// GDI Font constructor would silently substitute a text face for a
    /// missing family (tofu), hence the explicit installed-font probe.
    /// </summary>
    private static readonly Lazy<string> GlyphFontFamily = new(() =>
    {
        using var installedFonts = new InstalledFontCollection();
        var installed = installedFonts.Families
            .Select(family => family.Name)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        foreach (var family in BrowserTheme.GlyphFontFamilies)
        {
            if (installed.Contains(family))
            {
                return family;
            }
        }

        // No icon font at all (non-Windows dev host): glyphs degrade to
        // visible placeholder boxes in the UI font instead of crashing.
        return BrowserTheme.UiFontFamily;
    });

    private void OnAddressKeyDown(object? sender, KeyEventArgs args)
    {
        if (args.KeyCode != Keys.Enter)
        {
            return;
        }

        args.Handled = true;
        args.SuppressKeyPress = true;
        SubmitAddress();
    }

    private void SubmitAddress()
    {
        AddressRequested?.Invoke(this, new BrowserAddressRequestedEventArgs(addressBox.Text));
    }
}

internal sealed class BrowserAddressRequestedEventArgs : EventArgs
{
    public BrowserAddressRequestedEventArgs(string address)
    {
        Address = address;
    }

    public string Address { get; }
}

/// <summary>
/// Fully owner-drawn flat button: system-styled buttons cannot follow the
/// shared palette (they always render light), so normal/hover/pressed and
/// disabled states are painted here from the theme instead.
/// </summary>
internal sealed class BrowserThemedButton : Button
{
    private BrowserTheme theme = BrowserTheme.Light;
    private bool isHovered;
    private bool isPressed;
    private Color? restingBackground;
    private Color? enabledForeground;

    public BrowserThemedButton()
    {
        FlatStyle = FlatStyle.Flat;
        FlatAppearance.BorderSize = 0;
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint, true);
    }

    /// <summary>
    /// Resting-surface override for buttons that sit on a non-bar surface,
    /// e.g. the bookmark star inside the address input.
    /// </summary>
    public Color? RestingBackground
    {
        get => restingBackground;
        set
        {
            restingBackground = value;
            Invalidate();
        }
    }

    /// <summary>
    /// Enabled-text override, e.g. the accent-filled bookmark star; null
    /// falls back to the theme bar foreground.
    /// </summary>
    public Color? EnabledForeground
    {
        get => enabledForeground;
        set
        {
            enabledForeground = value;
            Invalidate();
        }
    }

    public void ApplyTheme(BrowserTheme theme)
    {
        this.theme = theme;
        BackColor = theme.BarBackground;
        ForeColor = theme.BarForeground;
        Invalidate();
    }

    protected override void OnMouseEnter(EventArgs args)
    {
        isHovered = true;
        base.OnMouseEnter(args);
        Invalidate();
    }

    protected override void OnMouseLeave(EventArgs args)
    {
        isHovered = false;
        isPressed = false;
        base.OnMouseLeave(args);
        Invalidate();
    }

    protected override void OnMouseDown(MouseEventArgs args)
    {
        if (args.Button == MouseButtons.Left)
        {
            isPressed = true;
        }

        base.OnMouseDown(args);
        Invalidate();
    }

    protected override void OnMouseUp(MouseEventArgs args)
    {
        isPressed = false;
        base.OnMouseUp(args);
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs args)
    {
        var restingColor = restingBackground ?? theme.BarBackground;
        var background = !Enabled
            ? restingColor
            : isPressed
                ? theme.ControlPressed
                : isHovered
                    ? theme.ControlHover
                    : restingColor;
        args.Graphics.Clear(background);

        var textColor = Enabled
            ? enabledForeground ?? theme.BarForeground
            : theme.DisabledForeground;
        TextRenderer.DrawText(
            args.Graphics,
            Text,
            Font,
            ClientRectangle,
            textColor,
            TextFormatFlags.HorizontalCenter
            | TextFormatFlags.VerticalCenter
            | TextFormatFlags.EndEllipsis
            | TextFormatFlags.NoPrefix);

        if (Focused && ShowFocusCues)
        {
            var focusBounds = ClientRectangle;
            focusBounds.Inflate(-2, -2);
            ControlPaint.DrawFocusRectangle(args.Graphics, focusBounds, textColor, background);
        }
    }
}

/// <summary>
/// One-pixel theme-painted border around a borderless TextBox: the system
/// FixedSingle border cannot follow the palette, this panel can. The border
/// switches to the accent while the inner box has focus. An optional
/// trailing control (the toolbar's bookmark star) docks inside the right
/// edge and the text area shrinks around it.
/// </summary>
internal sealed class BrowserThemedBorderPanel : Panel
{
    private readonly TextBox innerBox;
    private readonly Control? trailingControl;
    private BrowserTheme theme = BrowserTheme.Light;

    public BrowserThemedBorderPanel(TextBox innerBox, Control? trailingControl = null)
    {
        this.innerBox = innerBox;
        this.trailingControl = trailingControl;
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint, true);
        Controls.Add(innerBox);
        if (trailingControl is not null)
        {
            Controls.Add(trailingControl);
        }

        innerBox.GotFocus += (_, _) => Invalidate();
        innerBox.LostFocus += (_, _) => Invalidate();
        Resize += (_, _) => LayoutInnerBox();
        LayoutInnerBox();
    }

    public void ApplyTheme(BrowserTheme theme)
    {
        this.theme = theme;
        BackColor = theme.InputBackground;
        innerBox.BackColor = theme.InputBackground;
        innerBox.ForeColor = theme.InputForeground;
        LayoutInnerBox();
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs args)
    {
        base.OnPaint(args);
        var borderColor = innerBox.Focused ? theme.Accent : theme.Border;
        var thickness = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
        using var pen = new Pen(borderColor, thickness);
        var inset = thickness / 2f;
        args.Graphics.DrawRectangle(
            pen,
            inset,
            inset,
            Width - thickness,
            Height - thickness);
    }

    private void LayoutInnerBox()
    {
        var horizontalPadding = BrowserTheme.Metrics.Scale(8, DeviceDpi);
        var trailingWidth = 0;
        if (trailingControl is not null)
        {
            var borderThickness = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.BorderThickness, DeviceDpi);
            var side = Math.Max(0, Height - (borderThickness * 2));
            trailingControl.SetBounds(
                Math.Max(0, Width - borderThickness - side),
                borderThickness,
                side,
                side);
            trailingWidth = side;
        }

        innerBox.Location = new Point(
            horizontalPadding,
            Math.Max(0, (Height - innerBox.PreferredHeight) / 2));
        innerBox.Width = Math.Max(0, Width - (horizontalPadding * 2) - trailingWidth);
    }
}
