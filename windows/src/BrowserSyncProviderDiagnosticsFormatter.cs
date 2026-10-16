using System.Text.Json;
using System.Text.Json.Nodes;

namespace Actrium.Windows;

// Renders the collapsible Diagnostics detail of the Browser Data Sync
// settings dialog from the last provider round envelope the sync coordinator
// keeps in memory. The Rust core owns the diagnostic vocabulary — every
// local-folder round envelope embeds the Rust-built providerDiagnostics
// summary and providerErrorClassification plan — while BrowserStrings owns
// the user-facing labels. Contract tokens (classifications, actions, error
// kinds, object paths) render raw on purpose: the detail is a faithful
// diagnosis for support and development, not a translation layer, and a
// contract addition must stay visible instead of degrading to a generic
// label. Harness-linked: this file must stay WinForms-free.
internal static class BrowserSyncProviderDiagnosticsFormatter
{
    private const string Indent = "  ";

    public static string FormatFromJson(string? roundEnvelopeJson)
    {
        if (string.IsNullOrWhiteSpace(roundEnvelopeJson))
        {
            return BrowserStrings.Get("diagnostics.syncProvider.empty");
        }

        try
        {
            return Format(JsonNode.Parse(roundEnvelopeJson) as JsonObject);
        }
        catch (JsonException)
        {
            // A malformed summary means no trustworthy diagnosis exists;
            // showing the empty state is more honest than a partial render.
            return BrowserStrings.Get("diagnostics.syncProvider.empty");
        }
    }

    public static string Format(JsonObject? roundEnvelope)
    {
        var providerDiagnostics = roundEnvelope?["providerDiagnostics"] as JsonObject;
        var errorClassification = roundEnvelope?["providerErrorClassification"] as JsonObject;
        if (providerDiagnostics is null && errorClassification is null)
        {
            return BrowserStrings.Get("diagnostics.syncProvider.empty");
        }

        var lines = new List<string>();
        if (providerDiagnostics is not null)
        {
            AppendDiagnosticsSummary(lines, providerDiagnostics);
        }

        if (errorClassification is not null)
        {
            AppendErrorClassification(lines, errorClassification);
        }

        return string.Join('\n', lines);
    }

    private static void AppendDiagnosticsSummary(List<string> lines, JsonObject diagnostics)
    {
        lines.Add(BrowserStrings.Get("diagnostics.syncProvider.summarySection"));
        AddRow(lines, 1, "diagnostics.syncProvider.classificationRow", JsonString(diagnostics, "classification"));
        AddRow(lines, 1, "diagnostics.syncProvider.severityRow", JsonString(diagnostics, "severity"));
        AddRow(lines, 1, "diagnostics.syncProvider.providerRow", JsonString(diagnostics, "providerKind"));
        AddBoolRow(lines, 1, "diagnostics.syncProvider.syncAllowedRow", JsonBool(diagnostics, "shouldAllowSync"));
        AddBoolRow(lines, 1, "diagnostics.syncProvider.retryRow", JsonBool(diagnostics, "shouldRetry"));
        AddBoolRow(lines, 1, "diagnostics.syncProvider.rereadRow", JsonBool(diagnostics, "shouldReread"));
        AddActionRow(lines, 1, JsonString(diagnostics, "userAction"));
        AddListRows(lines, 1, "diagnostics.syncProvider.reasonsRow", diagnostics["diagnosticReasons"] as JsonArray);
        AddListRows(lines, 1, "diagnostics.syncProvider.warningsRow", diagnostics["warnings"] as JsonArray);
    }

    private static void AppendErrorClassification(List<string> lines, JsonObject classification)
    {
        lines.Add(BrowserStrings.Get("diagnostics.syncProvider.errorSection"));
        AddRow(lines, 1, "diagnostics.syncProvider.classificationRow", JsonString(classification, "classification"));
        AddRow(
            lines,
            1,
            "diagnostics.syncProvider.failedOperationsRow",
            BrowserStrings.Format(
                "diagnostics.syncProvider.failedOperationsFormat",
                JsonInt(classification, "failedCount"),
                JsonInt(classification, "operationCount"),
                JsonInt(classification, "retryableFailureCount")));
        AddBoolRow(lines, 1, "diagnostics.syncProvider.retryRow", JsonBool(classification, "shouldRetry"));
        AddBoolRow(lines, 1, "diagnostics.syncProvider.rereadRow", JsonBool(classification, "shouldRereadCloud"));
        AddActionRow(lines, 1, JsonString(classification, "userAction"));

        if (classification["errors"] is not JsonArray errors || errors.Count == 0)
        {
            return;
        }

        lines.Add(RowText(1, "diagnostics.syncProvider.errorsRow"));
        foreach (var entry in errors)
        {
            if (entry is not JsonObject error)
            {
                continue;
            }

            // Headline: the normalized operation plus the object path the
            // provider failed on; the classified detail rows follow indented.
            var operationType = JsonString(error, "operationType") ?? string.Empty;
            var path = JsonString(error, "path");
            var headline = string.IsNullOrEmpty(path) ? operationType : $"{operationType} {path}";
            lines.Add(IndentText(2) + BrowserStrings.Format("diagnostics.syncProvider.listItemFormat", headline));
            AddRow(lines, 3, "diagnostics.syncProvider.errorKindRow", JsonString(error, "canonicalErrorKind"));
            AddRow(lines, 3, "diagnostics.syncProvider.errorActionRow", JsonString(error, "action"));
            AddRow(lines, 3, "diagnostics.syncProvider.errorReasonRow", JsonString(error, "diagnosticReason"));
            AddRow(lines, 3, "diagnostics.syncProvider.errorMessageRow", JsonString(error, "message"));
        }
    }

    private static void AddRow(List<string> lines, int depth, string labelKey, object? value)
    {
        var text = value?.ToString();
        if (string.IsNullOrEmpty(text))
        {
            return;
        }

        lines.Add(IndentText(depth) + BrowserStrings.Format(
            "diagnostics.syncProvider.rowFormat",
            BrowserStrings.Get(labelKey),
            text));
    }

    private static void AddBoolRow(List<string> lines, int depth, string labelKey, bool value)
    {
        AddRow(
            lines,
            depth,
            labelKey,
            BrowserStrings.Get(value ? "diagnostics.syncProvider.yes" : "diagnostics.syncProvider.no"));
    }

    private static void AddActionRow(List<string> lines, int depth, string? userAction)
    {
        // "none" is the contract's explicit no-action token; rendering it
        // would read as an action named "none".
        if (string.IsNullOrEmpty(userAction) || string.Equals(userAction, "none", StringComparison.Ordinal))
        {
            return;
        }

        AddRow(lines, depth, "diagnostics.syncProvider.userActionRow", userAction);
    }

    private static void AddListRows(List<string> lines, int depth, string labelKey, JsonArray? items)
    {
        if (items is null)
        {
            return;
        }

        var values = new List<string>();
        foreach (var item in items)
        {
            if (item is JsonValue value
                && value.TryGetValue<string>(out var text)
                && !string.IsNullOrEmpty(text))
            {
                values.Add(text);
            }
        }

        if (values.Count == 0)
        {
            return;
        }

        lines.Add(RowText(depth, labelKey));
        foreach (var value in values)
        {
            lines.Add(IndentText(depth + 1)
                + BrowserStrings.Format("diagnostics.syncProvider.listItemFormat", value));
        }
    }

    private static string RowText(int depth, string labelKey)
    {
        return IndentText(depth) + BrowserStrings.Format(
            "diagnostics.syncProvider.listHeaderFormat",
            BrowserStrings.Get(labelKey));
    }

    private static string IndentText(int depth)
    {
        return string.Concat(Enumerable.Repeat(Indent, depth));
    }

    private static string? JsonString(JsonObject source, string propertyName)
    {
        return source[propertyName] is JsonValue value && value.TryGetValue<string>(out var text)
            ? text
            : null;
    }

    private static bool JsonBool(JsonObject source, string propertyName)
    {
        return source[propertyName] is JsonValue value
            && value.TryGetValue<bool>(out var result)
            && result;
    }

    private static int JsonInt(JsonObject source, string propertyName)
    {
        return source[propertyName] is JsonValue value && value.TryGetValue<int>(out var result)
            ? result
            : 0;
    }
}
