using System.Drawing;
using System.Windows.Forms;

namespace Actro.Windows;

// Product surface for the browser sync provider connection, structured after
// macOS Settings > Sync: provider picker, sync folder, status, device, and a
// collapsible diagnostics detail over the last provider round's Rust
// diagnostics (BrowserSyncProviderDiagnosticsFormatter). Local
// Folder is the only enabled provider; the roadmap cloud providers appear as
// disabled "coming later" rows because they already classify through the
// Rust provider connection contract. The endpoint-token dialog
// (BrowserSyncConnectionDialog) remains reachable solely through the
// advanced reference-server link as the development-only protocol probe.
//
// Folder acceptance persists nothing until the BrowserSyncFolderPreflight
// pipeline passes (platform probe -> capability preflight -> adapter plan ->
// compatibility preflight); Disconnect stops browser sync entirely (provider
// fields and the endpoint credential session) so leftover reference-server
// credentials cannot silently reactivate endpoint sync.
internal sealed class BrowserSyncSettingsDialog : Form
{
    // Marks section header labels so the theme pass can dim them without a
    // dedicated control subclass.
    private const string SectionHeaderTag = "sectionHeader";

    private readonly BrowserSyncProviderSettingsStore providerSettingsStore;
    private readonly BrowserStorageSyncCredentialStore endpointCredentialStore;
    private readonly Func<string?> readSchedulerStatusJson;
    private readonly Func<string?> readLastProviderDiagnosticsJson;
    private readonly Func<Task> runSyncNowAsync;
    private readonly Action scheduleSyncAfterConnect;
    private readonly Action cancelPendingSync;
    private readonly Action<IWin32Window> openReferenceServerDialog;

    private readonly RadioButton localFolderRadio = new();
    private readonly TextBox folderPathBox = new();
    private readonly BrowserThemedButton chooseFolderButton = new();
    private readonly BrowserThemedButton disconnectButton = new();
    private readonly Label folderErrorLabel = new();
    private readonly Label connectionValueLabel = new();
    private readonly Label lastSyncValueLabel = new();
    private readonly Label deviceValueLabel = new();
    private readonly BrowserThemedButton syncNowButton = new();
    private readonly BrowserThemedButton refreshStatusButton = new();
    private readonly BrowserThemedButton toggleDiagnosticsButton = new();
    private readonly TextBox diagnosticsBox = new();

    public BrowserSyncSettingsDialog(
        BrowserSyncProviderSettingsStore providerSettingsStore,
        BrowserStorageSyncCredentialStore endpointCredentialStore,
        Func<string?> readSchedulerStatusJson,
        Func<string?> readLastProviderDiagnosticsJson,
        Func<Task> runSyncNowAsync,
        Action scheduleSyncAfterConnect,
        Action cancelPendingSync,
        Action<IWin32Window> openReferenceServerDialog)
    {
        this.providerSettingsStore = providerSettingsStore;
        this.endpointCredentialStore = endpointCredentialStore;
        this.readSchedulerStatusJson = readSchedulerStatusJson;
        this.readLastProviderDiagnosticsJson = readLastProviderDiagnosticsJson;
        this.runSyncNowAsync = runSyncNowAsync;
        this.scheduleSyncAfterConnect = scheduleSyncAfterConnect;
        this.cancelPendingSync = cancelPendingSync;
        this.openReferenceServerDialog = openReferenceServerDialog;

        Text = BrowserStrings.Get("dialog.syncSettings.title");
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        MinimizeBox = false;
        MaximizeBox = false;
        ShowInTaskbar = false;
        AutoSize = true;
        AutoSizeMode = AutoSizeMode.GrowAndShrink;

        Controls.Add(BuildLayout());
        ApplyTheme(BrowserThemeManager.Instance.Current);
        RefreshFromSettings();
    }

    // All fixed layout metrics in this dialog are 96-DPI base values and
    // must resolve through this helper: the dialog opens with per-monitor
    // DPI applied to fonts, and unscaled pixel widths wrap the row labels
    // mid-word and clip buttons on scaled displays (seen at 125% on real
    // hardware). Like the theme pass, the DPI is read once at construction;
    // the dialog is modal and short-lived, so per-monitor moves are not
    // rescaled live.
    private int Scale(int baseValue)
    {
        return BrowserTheme.Metrics.Scale(baseValue, DeviceDpi);
    }

    private Control BuildLayout()
    {
        var layout = new TableLayoutPanel
        {
            AutoSize = true,
            AutoSizeMode = AutoSizeMode.GrowAndShrink,
            Padding = new Padding(Scale(16)),
            ColumnCount = 2,
        };
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, Scale(96)));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, Scale(440)));

        // 1. Provider section: Local Folder is the enabled provider; the
        // cloud drive rows are shown-but-disabled roadmap providers.
        AddSectionHeader(layout, BrowserStrings.Get("dialog.syncSettings.providerSection"));

        localFolderRadio.Text = BrowserStrings.Get("dialog.syncSettings.providerLocalFolder");
        localFolderRadio.AutoSize = true;
        localFolderRadio.CheckedChanged += (_, _) => ApplyFolderSectionEnabledState();
        AddFullRow(layout, localFolderRadio);

        foreach (var providerKey in new[]
                 {
                     "dialog.syncSettings.providerICloudDrive",
                     "dialog.syncSettings.providerGoogleDrive",
                     "dialog.syncSettings.providerOneDrive",
                 })
        {
            var comingLaterRadio = new RadioButton
            {
                Text = BrowserStrings.Format(
                    "dialog.syncSettings.providerDisabledFormat",
                    BrowserStrings.Get(providerKey),
                    BrowserStrings.Get("dialog.syncSettings.providerComingLater")),
                AutoSize = true,
                Enabled = false,
            };
            AddFullRow(layout, comingLaterRadio);
        }

        var providerFooterLabel = new Label
        {
            Text = BrowserStrings.Get("dialog.syncSettings.providerFooter"),
            AutoSize = true,
            MaximumSize = new Size(Scale(520), 0),
            Margin = new Padding(3, 6, 3, 10),
        };
        AddFullRow(layout, providerFooterLabel);

        // 2. Local Folder section: read-only path, Choose Folder... gated by
        // the preflight pipeline, Disconnect with full stop-sync semantics.
        AddSectionHeader(layout, BrowserStrings.Get("dialog.syncSettings.folderSection"));

        folderPathBox.ReadOnly = true;
        folderPathBox.Width = Scale(430);
        AddLabelledRow(layout, BrowserStrings.Get("dialog.syncSettings.folderRow"), folderPathBox);

        chooseFolderButton.Text = BrowserStrings.Get("dialog.syncSettings.chooseFolder");
        chooseFolderButton.AutoSize = true;
        chooseFolderButton.Click += (_, _) => ChooseFolder();
        disconnectButton.Text = BrowserStrings.Get("dialog.syncSettings.disconnect");
        disconnectButton.AutoSize = true;
        disconnectButton.Click += (_, _) => Disconnect();
        AddFullRow(layout, BuildButtonRow(chooseFolderButton, disconnectButton));

        folderErrorLabel.AutoSize = true;
        folderErrorLabel.MaximumSize = new Size(Scale(520), 0);
        folderErrorLabel.Visible = false;
        folderErrorLabel.Margin = new Padding(3, 0, 3, 10);
        AddFullRow(layout, folderErrorLabel);

        // 3. Status section: the provider connection row and the scheduler's
        // last-sync row, plus Sync Now / Refresh Status.
        AddSectionHeader(layout, BrowserStrings.Get("dialog.syncSettings.statusSection"));

        connectionValueLabel.AutoSize = true;
        connectionValueLabel.MaximumSize = new Size(Scale(430), 0);
        AddLabelledRow(layout, BrowserStrings.Get("dialog.syncSettings.connectionRow"), connectionValueLabel);

        lastSyncValueLabel.AutoSize = true;
        lastSyncValueLabel.MaximumSize = new Size(Scale(430), 0);
        AddLabelledRow(layout, BrowserStrings.Get("dialog.syncSettings.lastSyncRow"), lastSyncValueLabel);

        // 4. Device row: the durable device identity owned by the provider
        // settings store.
        deviceValueLabel.AutoSize = true;
        deviceValueLabel.MaximumSize = new Size(Scale(430), 0);
        AddLabelledRow(layout, BrowserStrings.Get("dialog.syncSettings.deviceRow"), deviceValueLabel);

        syncNowButton.Text = BrowserStrings.Get("dialog.syncSettings.syncNow");
        syncNowButton.AutoSize = true;
        syncNowButton.Click += OnSyncNowClicked;
        refreshStatusButton.Text = BrowserStrings.Get("dialog.syncSettings.refreshStatus");
        refreshStatusButton.AutoSize = true;
        refreshStatusButton.Click += (_, _) => RefreshStatusRows();
        AddFullRow(layout, BuildButtonRow(syncNowButton, refreshStatusButton));

        // Collapsible diagnostics detail: the last provider round envelope's
        // Rust diagnostics and error classification, rendered readably by
        // the pure formatter. Hidden by default — it is a support detail,
        // not a status row — and refreshed with the status rows while open.
        toggleDiagnosticsButton.Text = BrowserStrings.Get("dialog.syncSettings.showDiagnostics");
        toggleDiagnosticsButton.AutoSize = true;
        toggleDiagnosticsButton.Click += (_, _) => ToggleDiagnostics();
        AddFullRow(layout, BuildButtonRow(toggleDiagnosticsButton));

        diagnosticsBox.Multiline = true;
        diagnosticsBox.ReadOnly = true;
        diagnosticsBox.WordWrap = false;
        diagnosticsBox.ScrollBars = ScrollBars.Both;
        diagnosticsBox.Width = Scale(520);
        diagnosticsBox.Height = Scale(160);
        diagnosticsBox.Visible = false;
        diagnosticsBox.Margin = new Padding(3, 0, 3, 4);
        AddFullRow(layout, diagnosticsBox);

        // 5. Bottom row: the advanced reference-server (development) link on
        // the left, Close on the right.
        var referenceServerButton = new BrowserThemedButton
        {
            Text = BrowserStrings.Get("dialog.syncSettings.referenceServerLink"),
            AutoSize = true,
        };
        referenceServerButton.Click += (_, _) =>
        {
            openReferenceServerDialog(this);
            // A saved or cleared endpoint session changes the truthful
            // connection row (reference-server-active vs not-configured).
            RefreshStatusRows();
        };

        var closeButton = new BrowserThemedButton
        {
            Text = BrowserStrings.Get("dialog.syncSettings.close"),
            DialogResult = DialogResult.Cancel,
            AutoSize = true,
        };

        var bottomRow = new TableLayoutPanel
        {
            AutoSize = true,
            AutoSizeMode = AutoSizeMode.GrowAndShrink,
            ColumnCount = 2,
            Margin = new Padding(0, 12, 0, 0),
        };
        bottomRow.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, Scale(320)));
        bottomRow.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, Scale(216)));
        referenceServerButton.Anchor = AnchorStyles.Left;
        closeButton.Anchor = AnchorStyles.Right;
        bottomRow.Controls.Add(referenceServerButton, 0, 0);
        bottomRow.Controls.Add(closeButton, 1, 0);
        AddFullRow(layout, bottomRow);

        CancelButton = closeButton;
        return layout;
    }

    private static void AddSectionHeader(TableLayoutPanel layout, string text)
    {
        var header = new Label
        {
            Text = text,
            AutoSize = true,
            Margin = new Padding(0, 10, 3, 4),
            Tag = SectionHeaderTag,
        };
        AddFullRow(layout, header);
    }

    private static void AddFullRow(TableLayoutPanel layout, Control control)
    {
        var row = layout.RowCount;
        layout.RowCount = row + 1;
        layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.Controls.Add(control, 0, row);
        layout.SetColumnSpan(control, 2);
    }

    private static void AddLabelledRow(TableLayoutPanel layout, string labelText, Control value)
    {
        var row = layout.RowCount;
        layout.RowCount = row + 1;
        layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        var label = new Label
        {
            Text = labelText,
            AutoSize = true,
            Margin = new Padding(3, 6, 3, 3),
        };
        layout.Controls.Add(label, 0, row);
        layout.Controls.Add(value, 1, row);
    }

    private static Control BuildButtonRow(params Control[] buttons)
    {
        var row = new FlowLayoutPanel
        {
            AutoSize = true,
            AutoSizeMode = AutoSizeMode.GrowAndShrink,
            FlowDirection = FlowDirection.LeftToRight,
            Margin = new Padding(0, 2, 0, 2),
        };
        foreach (var button in buttons)
        {
            row.Controls.Add(button);
        }

        return row;
    }

    // One theme source: palette and typography come from BrowserTheme via
    // the theme manager; the dialog is modal and short-lived, so it reads
    // the current theme at construction instead of subscribing to changes.
    private void ApplyTheme(BrowserTheme theme)
    {
        BackColor = theme.WindowBackground;
        ForeColor = theme.BarForeground;
        Font = BrowserThemeManager.Instance.GetUiFont(DeviceDpi);

        folderPathBox.BackColor = theme.InputBackground;
        folderPathBox.ForeColor = theme.InputForeground;
        diagnosticsBox.BackColor = theme.InputBackground;
        diagnosticsBox.ForeColor = theme.InputForeground;
        folderErrorLabel.ForeColor = theme.Accent;
        connectionValueLabel.ForeColor = theme.BarForeground;
        lastSyncValueLabel.ForeColor = theme.BarForegroundMuted;
        deviceValueLabel.ForeColor = theme.BarForegroundMuted;

        ApplyThemeToChildren(this, theme);
    }

    private static void ApplyThemeToChildren(Control parent, BrowserTheme theme)
    {
        foreach (Control child in parent.Controls)
        {
            switch (child)
            {
                case BrowserThemedButton themedButton:
                    themedButton.ApplyTheme(theme);
                    themedButton.RestingBackground = theme.TabActiveBackground;
                    break;
                case Label label when Equals(label.Tag, SectionHeaderTag):
                    label.ForeColor = theme.BarForegroundMuted;
                    break;
                case RadioButton radio:
                    radio.ForeColor = radio.Enabled
                        ? theme.BarForeground
                        : theme.DisabledForeground;
                    break;
            }

            ApplyThemeToChildren(child, theme);
        }
    }

    private void RefreshFromSettings()
    {
        var settings = providerSettingsStore.Load();
        var localFolderConfigured = settings.ProviderKind
            == BrowserSyncProviderSettingsStore.LocalFolderProviderKind
            && !string.IsNullOrEmpty(settings.LocalFolderRootPath);
        localFolderRadio.Checked = localFolderConfigured;
        folderPathBox.Text = localFolderConfigured
            ? settings.LocalFolderRootPath
            : BrowserStrings.Get("dialog.syncSettings.folderNotSelected");
        deviceValueLabel.Text = settings.DeviceId;
        ApplyFolderSectionEnabledState();
        RefreshStatusRows();
    }

    private void ApplyFolderSectionEnabledState()
    {
        // The folder affordances follow the picker selection; Disconnect
        // additionally requires something to disconnect from.
        chooseFolderButton.Enabled = localFolderRadio.Checked;
        disconnectButton.Enabled = localFolderRadio.Checked
            && BrowserSyncProviderConnectionPlanner.IsConfigured(providerSettingsStore.Load());
    }

    private void RefreshStatusRows()
    {
        var settings = providerSettingsStore.Load();
        string connectionText;
        if (BrowserSyncProviderConnectionPlanner.IsConfigured(settings))
        {
            // Same snapshot assembly and Rust policy as the coordinator's
            // provider branch, evaluated fresh for display.
            var plan = BrowserSyncProviderConnectionPlanner.Plan(
                settings,
                lastProviderErrorKind: null,
                retryAttempt: 0,
                DateTimeOffset.UtcNow.ToUnixTimeSeconds());
            connectionText = BrowserSyncProviderStatusFormatter.FormatConnectionPolicy(
                plan.ConnectionPolicy);
        }
        else if (HasEndpointSession())
        {
            connectionText = BrowserSyncProviderStatusFormatter.FormatReferenceServerActive();
        }
        else
        {
            connectionText = BrowserSyncProviderStatusFormatter.FormatNotConfigured();
        }

        connectionValueLabel.Text = connectionText;
        lastSyncValueLabel.Text = BrowserSyncStatusPolicy.FormatStatusJson(readSchedulerStatusJson());
        if (diagnosticsBox.Visible)
        {
            RefreshDiagnostics();
        }
    }

    private void ToggleDiagnostics()
    {
        diagnosticsBox.Visible = !diagnosticsBox.Visible;
        toggleDiagnosticsButton.Text = BrowserStrings.Get(diagnosticsBox.Visible
            ? "dialog.syncSettings.hideDiagnostics"
            : "dialog.syncSettings.showDiagnostics");
        if (diagnosticsBox.Visible)
        {
            RefreshDiagnostics();
        }
    }

    private void RefreshDiagnostics()
    {
        // The formatter is pure and newline-based; the WinForms TextBox
        // needs platform line endings to actually break lines.
        diagnosticsBox.Text = BrowserSyncProviderDiagnosticsFormatter
            .FormatFromJson(readLastProviderDiagnosticsJson())
            .Replace("\n", Environment.NewLine, StringComparison.Ordinal);
    }

    private bool HasEndpointSession()
    {
        try
        {
            return endpointCredentialStore.Load() is not null;
        }
        catch (InvalidOperationException)
        {
            // A corrupt endpoint credential file is still an endpoint session
            // artifact on disk; claiming "not configured" would hide it.
            return true;
        }
    }

    private void ChooseFolder()
    {
        using var folderBrowser = new FolderBrowserDialog
        {
            Description = BrowserStrings.Get("dialog.syncSettings.chooseFolderDescription"),
            UseDescriptionForTitle = true,
            ShowNewFolderButton = true,
        };
        var settings = providerSettingsStore.Load();
        if (!string.IsNullOrEmpty(settings.LocalFolderRootPath))
        {
            folderBrowser.SelectedPath = settings.LocalFolderRootPath;
        }

        if (folderBrowser.ShowDialog(this) != DialogResult.OK)
        {
            return;
        }

        AcceptFolder(folderBrowser.SelectedPath);
    }

    private void AcceptFolder(string rootPath)
    {
        var observedAt = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        var directoryExists = Directory.Exists(rootPath);
        var result = BrowserSyncFolderPreflight.Evaluate(
            directoryExists,
            directoryExists && RunWriteProbe(rootPath),
            ActroCore.BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson(observedAt),
            ActroCore.BuildBrowserStorageSyncCloudProviderAdapterPlanJson(
                ActroCore.LoadBrowserStorageSyncCloudLocalFolderCapabilitiesJson(),
                observedAt),
            ActroCore.BuildBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
                rootPath,
                observedAt));
        if (!result.Accept)
        {
            ShowFolderError(result);
            return;
        }

        folderErrorLabel.Visible = false;
        providerSettingsStore.Save(providerSettingsStore.CreateSnapshot(
            BrowserSyncProviderSettingsStore.LocalFolderProviderKind,
            rootPath));
        // The first scheduled run performs the membership join automatically:
        // the connection status policy classifies membershipRequired until
        // the persisted membership marker is set by the coordinator.
        scheduleSyncAfterConnect();
        RefreshFromSettings();
    }

    // The write probe is dialog-side IO by design: the preflight evaluator
    // stays pure, and each sync round re-validates the folder
    // authoritatively through its own preflights.
    private static bool RunWriteProbe(string rootPath)
    {
        var probePath = Path.Combine(rootPath, $".actro-sync-write-probe-{Guid.NewGuid():N}");
        try
        {
            File.WriteAllText(probePath, "probe");
            File.Delete(probePath);
            return true;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
        catch (NotSupportedException)
        {
            return false;
        }
    }

    private void ShowFolderError(BrowserSyncFolderPreflightResult result)
    {
        var reasonText = BrowserStrings.Get(
            $"dialog.syncSettings.folderBlocked.{result.BlockedReasonKey}");
        folderErrorLabel.Text = result.DetailText.Length > 0
            ? BrowserStrings.Format(
                "dialog.syncSettings.folderBlockedDetailFormat",
                reasonText,
                result.DetailText)
            : reasonText;
        folderErrorLabel.Visible = true;
    }

    private void Disconnect()
    {
        providerSettingsStore.DisconnectAndClearEndpointSession(endpointCredentialStore);
        cancelPendingSync();
        folderErrorLabel.Visible = false;
        RefreshFromSettings();
    }

    private async void OnSyncNowClicked(object? sender, EventArgs args)
    {
        // Same flow as the Sync Now menu command; the dialog stays open and
        // the status rows refresh on completion. The delegate owns its own
        // failure reporting.
        syncNowButton.Enabled = false;
        refreshStatusButton.Enabled = false;
        try
        {
            await runSyncNowAsync();
        }
        finally
        {
            syncNowButton.Enabled = true;
            refreshStatusButton.Enabled = true;
            RefreshStatusRows();
        }
    }
}
