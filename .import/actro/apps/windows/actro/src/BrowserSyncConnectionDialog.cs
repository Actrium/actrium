using System.Drawing;
using System.Windows.Forms;

namespace Actro.Windows;

internal sealed class BrowserSyncConnectionDialog : Form
{
    private readonly ComboBox environmentInput = new();
    private readonly TextBox baseUrlInput = new();
    private readonly TextBox accessTokenInput = new();
    private readonly TextBox deviceIdInput = new();
    private string lastAutoBaseUrl = string.Empty;

    public BrowserSyncConnectionDialog(BrowserStorageSyncCredentialSnapshot? snapshot)
    {
        Text = BrowserStrings.Get("dialog.sync.title");
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        MinimizeBox = false;
        MaximizeBox = false;
        ShowInTaskbar = false;
        ClientSize = new Size(520, 238);

        environmentInput.DropDownStyle = ComboBoxStyle.DropDownList;
        environmentInput.Items.AddRange(["prod", "beta", "dev"]);
        environmentInput.SelectedItem = NormalizeEnvironment(snapshot?.Environment);
        environmentInput.SelectedIndexChanged += (_, _) => ApplyEnvironmentDefaultBaseUrl();

        baseUrlInput.Text = snapshot?.BaseUrl ?? DefaultBaseUrl(Environment);
        lastAutoBaseUrl = DefaultBaseUrl(Environment);

        accessTokenInput.Text = snapshot?.AccessToken ?? string.Empty;
        accessTokenInput.UseSystemPasswordChar = true;

        deviceIdInput.Text = snapshot?.DeviceId ?? string.Empty;
        deviceIdInput.ReadOnly = true;

        var saveButton = new Button
        {
            Text = BrowserStrings.Get("dialog.sync.save"),
            DialogResult = DialogResult.None,
            Width = 88,
        };
        saveButton.Click += (_, _) => Save();

        var clearButton = new Button
        {
            Text = BrowserStrings.Get("dialog.sync.clear"),
            Width = 88,
        };
        clearButton.Click += (_, _) =>
        {
            ShouldClear = true;
            DialogResult = DialogResult.OK;
            Close();
        };

        var cancelButton = new Button
        {
            Text = BrowserStrings.Get("dialog.sync.cancel"),
            DialogResult = DialogResult.Cancel,
            Width = 88,
        };

        Controls.Add(BuildLayout(saveButton, clearButton, cancelButton));
        AcceptButton = saveButton;
        CancelButton = cancelButton;
    }

    public string Environment => (environmentInput.SelectedItem as string)?.Trim() ?? DefaultEnvironment();

    public string BaseUrl => baseUrlInput.Text.Trim();

    public string AccessToken => accessTokenInput.Text.Trim();

    public bool ShouldClear { get; private set; }

    private Control BuildLayout(Button saveButton, Button clearButton, Button cancelButton)
    {
        var layout = new TableLayoutPanel
        {
            Dock = DockStyle.Fill,
            Padding = new Padding(16),
            ColumnCount = 2,
            RowCount = 5,
        };
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 112));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));

        AddRow(layout, 0, BrowserStrings.Get("dialog.sync.environment"), environmentInput);
        AddRow(layout, 1, BrowserStrings.Get("dialog.sync.baseUrl"), baseUrlInput);
        AddRow(layout, 2, BrowserStrings.Get("dialog.sync.accessToken"), accessTokenInput);
        AddRow(layout, 3, BrowserStrings.Get("dialog.sync.deviceId"), deviceIdInput);

        var buttons = new FlowLayoutPanel
        {
            Dock = DockStyle.Fill,
            FlowDirection = FlowDirection.RightToLeft,
            Padding = new Padding(0, 12, 0, 0),
        };
        buttons.Controls.Add(cancelButton);
        buttons.Controls.Add(saveButton);
        buttons.Controls.Add(clearButton);
        layout.Controls.Add(buttons, 0, 4);
        layout.SetColumnSpan(buttons, 2);
        return layout;
    }

    private static void AddRow(TableLayoutPanel layout, int row, string label, Control input)
    {
        var labelControl = new Label
        {
            Text = label,
            Dock = DockStyle.Fill,
            TextAlign = ContentAlignment.MiddleLeft,
        };
        input.Dock = DockStyle.Fill;
        layout.Controls.Add(labelControl, 0, row);
        layout.Controls.Add(input, 1, row);
    }

    private void ApplyEnvironmentDefaultBaseUrl()
    {
        var nextBaseUrl = DefaultBaseUrl(Environment);
        if (string.IsNullOrWhiteSpace(baseUrlInput.Text)
            || string.Equals(baseUrlInput.Text.Trim(), lastAutoBaseUrl, StringComparison.Ordinal))
        {
            baseUrlInput.Text = nextBaseUrl;
        }

        lastAutoBaseUrl = nextBaseUrl;
    }

    private void Save()
    {
        if (string.IsNullOrWhiteSpace(AccessToken))
        {
            MessageBox.Show(
                this,
                BrowserStrings.Get("dialog.sync.accessTokenRequired"),
                BrowserStrings.Get("dialog.sync.title"),
                MessageBoxButtons.OK,
                MessageBoxIcon.Warning);
            return;
        }

        if (string.IsNullOrWhiteSpace(BaseUrl))
        {
            baseUrlInput.Text = DefaultBaseUrl(Environment);
        }

        ShouldClear = false;
        DialogResult = DialogResult.OK;
        Close();
    }

    private static string NormalizeEnvironment(string? environment)
    {
        var normalized = environment?.Trim() ?? string.Empty;
        return normalized is "dev" or "beta" or "prod" ? normalized : DefaultEnvironment();
    }

    // The Rust core owns the auth environment and account endpoint contract;
    // an empty value is contract drift and must fail loudly instead of
    // degrading to a hand-copied C# endpoint.
    private static string DefaultEnvironment()
    {
        var environment = ActroCore.LoadAuthEnvironmentProd();
        return string.IsNullOrWhiteSpace(environment)
            ? throw new InvalidOperationException(
                "Default auth environment from the Rust core is empty.")
            : environment.Trim();
    }

    private static string DefaultBaseUrl(string environment)
    {
        var baseUrl = ActroCore.LoadAuthAccountBaseUrl(environment);
        return string.IsNullOrWhiteSpace(baseUrl)
            ? throw new InvalidOperationException(
                $"Auth account base URL from the Rust core is empty for environment '{environment}'.")
            : baseUrl.Trim();
    }
}
