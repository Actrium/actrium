using System.Drawing;
using System.Windows.Forms;

namespace Actro.Windows;

internal sealed class BrowserToolbar : UserControl
{
    private readonly Button backButton;
    private readonly Button forwardButton;
    private readonly Button reloadButton;
    private readonly Button homeButton;
    private readonly TextBox addressBox;
    private readonly Button goButton;
    private readonly Label statusLabel;

    public event EventHandler? BackRequested;
    public event EventHandler? ForwardRequested;
    public event EventHandler? ReloadRequested;
    public event EventHandler? HomeRequested;
    public event EventHandler<BrowserAddressRequestedEventArgs>? AddressRequested;

    public BrowserToolbar()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        BackColor = Color.FromArgb(246, 247, 249);
        Dock = DockStyle.Top;
        Height = 44;
        MinimumSize = new Size(0, 44);
        Padding = new Padding(8, 7, 8, 7);

        backButton = CreateButton("Back", 60);
        forwardButton = CreateButton("Forward", 76);
        reloadButton = CreateButton("Reload", 72);
        homeButton = CreateButton("Home", 64);
        goButton = CreateButton("Go", 48);

        addressBox = new TextBox
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            BorderStyle = BorderStyle.FixedSingle,
            Font = new Font("Segoe UI", 10F, FontStyle.Regular, GraphicsUnit.Point),
            Margin = new Padding(6, 2, 6, 0),
        };

        statusLabel = new Label
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            AutoEllipsis = true,
            ForeColor = Color.FromArgb(89, 97, 109),
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
        layout.Controls.Add(addressBox, 4, 0);
        layout.Controls.Add(goButton, 5, 0);
        layout.Controls.Add(statusLabel, 6, 0);
        Controls.Add(layout);

        backButton.Click += (_, _) => BackRequested?.Invoke(this, EventArgs.Empty);
        forwardButton.Click += (_, _) => ForwardRequested?.Invoke(this, EventArgs.Empty);
        reloadButton.Click += (_, _) => ReloadRequested?.Invoke(this, EventArgs.Empty);
        homeButton.Click += (_, _) => HomeRequested?.Invoke(this, EventArgs.Empty);
        goButton.Click += (_, _) => SubmitAddress();
        addressBox.KeyDown += OnAddressKeyDown;

        SetNavigationState(canGoBack: false, canGoForward: false, isLoading: false);
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

    private static Button CreateButton(string text, int width)
    {
        return new Button
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            FlatStyle = FlatStyle.System,
            Height = 28,
            Margin = new Padding(2, 0, 2, 0),
            Text = text,
            Width = width,
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
