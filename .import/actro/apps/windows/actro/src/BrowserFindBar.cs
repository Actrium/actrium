using System.Drawing;
using System.Windows.Forms;

namespace Actro.Windows;

internal sealed class BrowserFindBar : UserControl
{
    private readonly TextBox queryBox;
    private readonly Button previousButton;
    private readonly Button nextButton;
    private readonly Button closeButton;

    public event EventHandler<BrowserFindRequestedEventArgs>? FindRequested;
    public event EventHandler? CloseRequested;

    public BrowserFindBar()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        BackColor = Color.FromArgb(250, 251, 252);
        Dock = DockStyle.Top;
        Height = 40;
        MinimumSize = new Size(0, 40);
        Padding = new Padding(8, 6, 8, 6);
        Visible = false;

        queryBox = new TextBox
        {
            Anchor = AnchorStyles.Left | AnchorStyles.Right,
            BorderStyle = BorderStyle.FixedSingle,
            Font = new Font("Segoe UI", 10F, FontStyle.Regular, GraphicsUnit.Point),
            Margin = new Padding(0, 1, 6, 0),
        };

        previousButton = CreateButton("Previous", 82);
        nextButton = CreateButton("Next", 58);
        closeButton = CreateButton("Close", 62);

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

        layout.Controls.Add(queryBox, 0, 0);
        layout.Controls.Add(previousButton, 1, 0);
        layout.Controls.Add(nextButton, 2, 0);
        layout.Controls.Add(closeButton, 3, 0);
        Controls.Add(layout);

        queryBox.KeyDown += OnQueryKeyDown;
        previousButton.Click += (_, _) => SubmitFind(forward: false);
        nextButton.Click += (_, _) => SubmitFind(forward: true);
        closeButton.Click += (_, _) => CloseRequested?.Invoke(this, EventArgs.Empty);
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
