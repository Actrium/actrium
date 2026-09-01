namespace Actro.Windows;

// The Rust core is the only source of the search engine catalog; an empty
// catalog, default id, or normalized id is contract drift and must fail
// loudly instead of degrading to a hand-copied C# engine list.
internal static class BrowserSearchEnginePolicy
{
    public static string DefaultEngineId
    {
        get
        {
            var defaultId = ActroCore.LoadBrowserSearchEngineDefaultId();
            return string.IsNullOrWhiteSpace(defaultId)
                ? throw new InvalidOperationException(
                    "Default browser search engine id from the Rust core is empty.")
                : defaultId;
        }
    }

    public static string DefaultSearchUrlPrefix
    {
        get
        {
            TryGetEngine(DefaultEngineId, out var engine);
            return engine.SearchUrlPrefix;
        }
    }

    public static IReadOnlyList<BrowserSearchEngineInfo> Engines
    {
        get
        {
            var engines = ActroCore.LoadBrowserSearchEngines();
            return engines.Count == 0
                ? throw new InvalidOperationException(
                    "Browser search engine catalog from the Rust core is empty.")
                : engines;
        }
    }

    public static string NormalizeEngineId(string? engineId)
    {
        var normalized = ActroCore.NormalizeBrowserSearchEngineId(engineId);
        return string.IsNullOrWhiteSpace(normalized)
            ? throw new InvalidOperationException(
                "Normalized browser search engine id from the Rust core is empty.")
            : normalized;
    }

    public static bool TryGetEngine(string? engineId, out BrowserSearchEngineInfo engine)
    {
        var normalizedEngineId = NormalizeEngineId(engineId);
        engine = Engines.First(candidate => candidate.Id == normalizedEngineId);
        return true;
    }

    public static string BuildSearchUrl(string query, string? engineId)
    {
        return ActroCore.BuildBrowserSearchUrl(query, engineId);
    }
}

internal sealed record BrowserSearchEngineInfo(
    string Id,
    string DisplayName,
    string SearchUrlPrefix);
