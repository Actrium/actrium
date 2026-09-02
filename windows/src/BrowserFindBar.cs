using System.Drawing;
using System.Windows.Forms;

namespace Actrium.Windows;

internal sealed class BrowserFindBar : UserControl
{
    private readonly BrowserThemedBorderPanel queryPanel;
    private readonly TextBox queryBox;
    private readonly BrowserThemedButton previousButton;
    private readonly BrowserThemedButton nextButton;
    private readonly BrowserThemedButton closeButton;

    public event EventHandler<BrowserFindRequestedEventArgs>? FindRequested;
    public event EventHandler? CloseRequested;

    public BrowserFindBar()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        Dock = DockStyle.Top;
        Padding = new Padding(8, 6, 8, 6);
        Visible = false;

        queryBox = new TextBox
        {
            BorderStyle = BorderStyle.None,
        };
        queryPanel = new BrowserThemedBorderPanel(queryBox)
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            Margin = new Padding(0, 0, 6, 0),
        };

        previousButton = CreateButton(BrowserStrings.Get("find.previous"));
        nextButton = CreateButton(BrowserStrings.Get("find.next"));
        closeButton = CreateButton(BrowserStrings.Get("find.close"));

        var layout = new TableLayoutPanel
        {
            ColumnCount = 4,
            Dock = DockStyle.Fill,
            Margin = Padding.Empty,
            Padding = Padding.Empty,
            RowCount = 1,
        };

        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 84));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 60));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 64));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));

        layout.Controls.Add(queryPanel, 0, 0);
        layout.Controls.Add(previousButton, 1, 0);
        layout.Controls.Add(nextButton, 2, 0);
        layout.Controls.Add(closeButton, 3, 0);
        Controls.Add(layout);

        queryBox.KeyDown += OnQueryKeyDown;
        previousButton.Click += (_, _) => SubmitFind(forward: false);
        nextButton.Click += (_, _) => SubmitFind(forward: true);
        closeButton.Click += (_, _) => CloseRequested?.Invoke(this, EventArgs.Empty);

        ApplyTheme(BrowserTheme.Light);
    }

    /// <summary>Applies palette, typography and DPI-scaled metrics in one pass.</summary>
    public void ApplyTheme(BrowserTheme theme)
    {
        var dpi = DeviceDpi;
        var barHeight = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.FindBarHeight, dpi);
        Height = barHeight;
        MinimumSize = new Size(0, barHeight);
        BackColor = theme.StatusBackground;

        var uiFont = BrowserThemeManager.Instance.GetUiFont(dpi);
        foreach (var button in new[] { previousButton, nextButton, closeButton })
        {
            button.Height = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.InputHeight, dpi);
            button.Font = uiFont;
            button.ApplyTheme(theme);
        }

        queryBox.Font = uiFont;
        queryPanel.Height = BrowserTheme.Metrics.Scale(BrowserTheme.Metrics.InputHeight, dpi);
        queryPanel.ApplyTheme(theme);
    }

    public void ShowAndFocus()
    {
        Visible = true;
        queryBox.Focus();
        queryBox.SelectAll();
    }

    public void SetQuery(string query)
    {
        queryBox.Text = query;
    }

    public string Query => queryBox.Text;

    private static BrowserThemedButton CreateButton(string text)
    {
        return new BrowserThemedButton
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            Margin = new Padding(2, 0, 2, 0),
            Text = text,
        };
    }

    private void OnQueryKeyDown(object? sender, KeyEventArgs args)
    {
        switch (args.KeyCode)
        {
            case Keys.Enter:
                args.Handled = true;
                args.SuppressKeyPress = true;
                SubmitFind(forward: !args.Shift);
                break;
            case Keys.Escape:
                args.Handled = true;
                args.SuppressKeyPress = true;
                CloseRequested?.Invoke(this, EventArgs.Empty);
                break;
        }
    }

    private void SubmitFind(bool forward)
    {
        FindRequested?.Invoke(this, new BrowserFindRequestedEventArgs(queryBox.Text, forward));
    }
}

internal sealed class BrowserFindRequestedEventArgs : EventArgs
{
    public BrowserFindRequestedEventArgs(string query, bool forward)
    {
        Query = query;
        Forward = forward;
    }

    public string Query { get; }

    public bool Forward { get; }
}
