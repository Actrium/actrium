using System.Drawing;
using System.Windows.Forms;

namespace Actro.Windows;

internal sealed class BrowserToolbar : UserControl
{
    private readonly BrowserThemedButton backButton;
    private readonly BrowserThemedButton forwardButton;
    private readonly BrowserThemedButton reloadButton;
    private readonly BrowserThemedButton homeButton;
    private readonly BrowserThemedBorderPanel addressPanel;
    private readonly TextBox addressBox;
    private readonly BrowserThemedButton goButton;
    private readonly Label statusLabel;

    public event EventHandler? BackRequested;
    public event EventHandler? ForwardRequested;
    public event EventHandler? ReloadRequested;
    public event EventHandler? HomeRequested;
    public event EventHandler<BrowserAddressRequestedEventArgs>? AddressRequested;

    public BrowserToolbar()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        Dock = DockStyle.Top;
        Padding = new Padding(8, 7, 8, 7);

        backButton = CreateButton("Back");
        forwardButton = CreateButton("Forward");
        reloadButton = CreateButton("Reload");
        homeButton = CreateButton("Home");
        goButton = CreateButton("Go");

        addressBox = new TextBox
        {
            BorderStyle = BorderStyle.None,
        };
        addressPanel = new BrowserThemedBorderPanel(addressBox)
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            Margin = new Padding(6, 0, 6, 0),
        };

        statusLabel = new Label
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            AutoEllipsis = true,
            Margin = new Padding(8, 4, 0, 0),
            TextAlign = ContentAlignment.MiddleLeft,
        };

        var layout = new TableLayoutPanel
        {
            ColumnCount = 7,
            Dock = DockStyle.Fill,
            Margin = Padding.Empty,
            Padding = Padding.Empty,
            RowCount = 1,
        };

        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 62));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 78));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 74));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 66));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 50));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 180));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));

        layout.Controls.Add(backButton, 0, 0);
        layout.Controls.Add(forwardButton, 1, 0);
        layout.Controls.Add(reloadButton, 2, 0);
        layout.Controls.Add(homeButton, 3, 0);
        layout.Controls.Add(addressPanel, 4, 0);
        layout.Controls.Add(goButton, 5, 0);
        layout.Controls.Add(statusLabel, 6, 0);
        Controls.Add(layout);

        backButton.Click += (_, _) => BackRequested?.Invoke(this, EventArgs.Empty);
        forwardButton.Click += (_, _) => ForwardRequested?.Invoke(this, EventArgs.Empty);
        reloadButton.Click += (_, _) => ReloadRequested?.Invoke(this, EventArgs.Empty);
        homeButton.Click += (_, _) => HomeRequested?.Invoke(this, EventArgs.Empty);
        goButton.Click += (_, _) => SubmitAddress();
        addressBox.KeyDown += OnAddressKeyDown;

        ApplyTheme(BrowserTheme.Light);
        SetNavigationState(canGoBack: false, canGoForward: false, isLoading: false);
    }

    /// <summary>Applies palette, typography and DPI-scaled metrics in one pass.</summary>
    public void ApplyTheme(BrowserTheme theme)
    {
        var dpi = DeviceDpi;
        var barHeight = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.ToolbarHeight, dpi);
        Height = barHeight;
        MinimumSize = new Size(0, barHeight);
        BackColor = theme.BarBackground;

        var uiFont = BrowserThemeManager.Instance.GetUiFont(dpi);
        foreach (var button in new[] { backButton, forwardButton, reloadButton, homeButton, goButton })
        {
            button.Height = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.InputHeight, dpi);
            button.Font = uiFont;
            button.ApplyTheme(theme);
        }

        addressBox.Font = uiFont;
        addressPanel.Height = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.InputHeight, dpi);
        addressPanel.ApplyTheme(theme);
        statusLabel.Font = uiFont;
        statusLabel.ForeColor = theme.BarForegroundMuted;
        statusLabel.BackColor = theme.BarBackground;
    }

    public void SetAddress(string address)
    {
        if (!addressBox.Focused)
        {
            addressBox.Text = address;
        }
    }

    public void SetNavigationState(bool canGoBack, bool canGoForward, bool isLoading)
    {
        backButton.Enabled = canGoBack;
        forwardButton.Enabled = canGoForward;
        reloadButton.Text = isLoading ? "Stop" : "Reload";
        statusLabel.Text = isLoading ? "Loading" : string.Empty;
    }

    public void SetStatus(string status)
    {
        statusLabel.Text = status;
    }

    public void FocusAddress()
    {
        addressBox.Focus();
        addressBox.SelectAll();
    }

    private static BrowserThemedButton CreateButton(string text)
    {
        return new BrowserThemedButton
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            Margin = new Padding(2, 0, 2, 0),
            Text = text,
        };
    }

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

    public BrowserThemedButton()
    {
        FlatStyle = FlatStyle.Flat;
        FlatAppearance.BorderSize = 0;
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint, true);
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
        var background = !Enabled
            ? theme.BarBackground
            : isPressed
                ? theme.ControlPressed
                : isHovered
                    ? theme.ControlHover
                    : theme.BarBackground;
        args.Graphics.Clear(background);

        var textColor = Enabled ? theme.BarForeground : theme.DisabledForeground;
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
/// switches to the accent while the inner box has focus.
/// </summary>
internal sealed class BrowserThemedBorderPanel : Panel
{
    private readonly TextBox innerBox;
    private BrowserTheme theme = BrowserTheme.Light;

    public BrowserThemedBorderPanel(TextBox innerBox)
    {
        this.innerBox = innerBox;
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint, true);
        Controls.Add(innerBox);
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
        innerBox.Location = new Point(
            horizontalPadding,
            Math.Max(0, (Height - innerBox.PreferredHeight) / 2));
        innerBox.Width = Math.Max(0, Width - (horizontalPadding * 2));
    }
}
