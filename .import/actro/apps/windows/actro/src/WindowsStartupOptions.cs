using System.Collections.Generic;

namespace Actro.Windows;

internal sealed record WindowsStartupOptions(IReadOnlyList<string> InitialAddresses, bool IsPrivate = false)
{
    public WindowsStartupOptions(string? initialUrl, bool isPrivate = false)
        : this(
            string.IsNullOrWhiteSpace(initialUrl) ? [] : [initialUrl],
            isPrivate)
    {
    }

    public string? InitialUrl => InitialAddresses.FirstOrDefault();

    public static WindowsStartupOptions FromArgs(IReadOnlyList<string> args)
    {
        var initialAddresses = new List<string>();
        var isPrivate = false;

        for (var index = 0; index < args.Count; index++)
        {
            if (args[index] == "--url" && index + 1 < args.Count)
            {
                initialAddresses.Add(args[index + 1]);
                index++;
                continue;
            }

            if (args[index] is "--private" or "--private-window" or "--incognito")
            {
                isPrivate = true;
                continue;
            }

            if (!args[index].StartsWith("-", StringComparison.Ordinal)
                && !string.IsNullOrWhiteSpace(args[index]))
            {
                initialAddresses.Add(args[index]);
            }
        }

        return new WindowsStartupOptions(initialAddresses, isPrivate);
    }
}
