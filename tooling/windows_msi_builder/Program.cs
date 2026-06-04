using System.Diagnostics;
using System.Globalization;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Xml.Linq;

var exitCode = await WindowsMsiBuilder.RunAsync(args);
return exitCode;

internal static class WindowsMsiBuilder
{
    private const string WixUiExtensionId = "WixToolset.UI.wixext";
    private const string WixUiExtensionVersion = "7.0.0";
    private const string ConptyUrl = "https://github.com/microsoft/terminal/releases/download/v1.23.13503.0/Microsoft.Windows.Console.ConPTY.1.23.251216003.nupkg";
    private const string ConptyArchiveName = "Microsoft.Windows.Console.ConPTY.1.23.251216003.nupkg";
    private const string AgsUrl = "https://codeload.github.com/GPUOpen-LibrariesAndSDKs/AGS_SDK/zip/refs/tags/v6.3.0";
    private const string AgsArchiveName = "AGS_SDK_v6.3.0.zip";
    private static readonly string[] SupportedLanguages = ["en-US", "zh-CN"];
    private static readonly HttpClient HttpClient = new();

    public static async Task<int> RunAsync(string[] args)
    {
        try
        {
            var options = Options.Parse(args);
            var workspaceRoot = ResolveWorkspaceRoot(options.WorkspaceRoot);
            var architecture = string.IsNullOrWhiteSpace(options.Architecture) ? "x86_64" : options.Architecture;
            var targetTriple = $"{architecture}-pc-windows-msvc";
            var releaseDirectory = Path.GetFullPath(options.ReleaseDirectory ?? ResolveReleaseDirectory(workspaceRoot, targetTriple));

            if (!Directory.Exists(releaseDirectory))
            {
                throw new InvalidOperationException($"Release directory was not found: {releaseDirectory}");
            }

            var mainBinaryPath = Path.Combine(releaseDirectory, "zed.exe");
            if (!File.Exists(mainBinaryPath))
            {
                throw new InvalidOperationException($"Expected release binary was not found: {mainBinaryPath}");
            }

            var channel = string.IsNullOrWhiteSpace(options.Channel)
                ? DiscoverReleaseChannel(workspaceRoot)
                : options.Channel.Trim();
            var channelConfig = ChannelConfig.For(channel);

            var version = string.IsNullOrWhiteSpace(options.Version)
                ? await DiscoverVersionAsync(workspaceRoot)
                : options.Version.Trim();
            var packageVersion = NormalizeMsiVersion(version);
            var languages = ResolveLanguages(options.Languages);

            var targetRoot = Path.Combine(workspaceRoot, "target", "windows-msi", architecture);
            var stagingDirectory = Path.GetFullPath(options.StagingDirectory ?? Path.Combine(targetRoot, "staging"));
            var generatedDirectory = Path.Combine(targetRoot, "generated");
            var cacheDirectory = Path.Combine(targetRoot, "cache");
            var outputPath = Path.GetFullPath(options.OutputPath ?? Path.Combine(workspaceRoot, "target", $"{channelConfig.OutputBaseName}-{architecture}-{packageVersion}.msi"));

            RecreateDirectory(stagingDirectory);
            RecreateDirectory(generatedDirectory);
            Directory.CreateDirectory(cacheDirectory);
            Directory.CreateDirectory(Path.GetDirectoryName(outputPath) ?? throw new InvalidOperationException("Output directory is invalid."));

            Console.WriteLine($"Staging files from {releaseDirectory}");
            var stagedFiles = await StageFilesAsync(workspaceRoot, releaseDirectory, stagingDirectory, cacheDirectory, architecture, channelConfig);

            Console.WriteLine("Generating WiX source");
            var fileAssociations = ParseFileAssociations(Path.Combine(workspaceRoot, "crates", "zed", "resources", "windows", "zed.iss"));
            var wixSourcePath = Path.Combine(generatedDirectory, "Zed.Generated.wxs");
            GenerateWixSource(
                wixSourcePath,
                stagedFiles,
                channelConfig,
                packageVersion,
                fileAssociations,
                options.IncludeDesktopShortcut,
                architecture);
            var localizationFilePaths = GenerateLocalizationFiles(generatedDirectory, channelConfig, architecture, languages);

            Console.WriteLine("Building MSI with WiX");
            RunProcess(
                "wix",
                ["eula", "accept", "wix7"],
                workspaceRoot,
                "WiX EULA acceptance",
                throwOnFailure: false,
                printOutput: false);
            var wixUiExtensionPath = EnsureWixUiExtension(workspaceRoot);
            var wixBuildArguments = new List<string>
            {
                "build",
                "-culture",
                languages[0],
                "-src",
                wixSourcePath,
                "-ext",
                wixUiExtensionPath,
            };
            foreach (var localizationFilePath in localizationFilePaths)
            {
                wixBuildArguments.Add("-loc");
                wixBuildArguments.Add(localizationFilePath);
            }

            wixBuildArguments.Add("-out");
            wixBuildArguments.Add(outputPath);
            RunProcess(
                "wix",
                wixBuildArguments,
                workspaceRoot,
                "WiX build");

            Console.WriteLine($"MSI created: {outputPath}");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception.Message);
            return 1;
        }
    }

    private static async Task<List<StagedFile>> StageFilesAsync(
        string workspaceRoot,
        string releaseDirectory,
        string stagingDirectory,
        string cacheDirectory,
        string architecture,
        ChannelConfig channelConfig)
    {
        var stagedFiles = new List<StagedFile>();
        var skipFileNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "cli.exe",
            "explorer_command_injector.dll",
            "remote_server.exe",
            ".cargo-lock",
        };

        foreach (var filePath in Directory.EnumerateFiles(releaseDirectory, "*", SearchOption.TopDirectoryOnly))
        {
            var fileName = Path.GetFileName(filePath);
            if (fileName.EndsWith(".pdb", StringComparison.OrdinalIgnoreCase) ||
                fileName.EndsWith(".lib", StringComparison.OrdinalIgnoreCase) ||
                fileName.EndsWith(".exp", StringComparison.OrdinalIgnoreCase) ||
                fileName.EndsWith(".d", StringComparison.OrdinalIgnoreCase) ||
                fileName.EndsWith(".zip", StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            if (skipFileNames.Contains(fileName))
            {
                continue;
            }

            var relativePath = Path.GetRelativePath(releaseDirectory, filePath);
            if (relativePath.Equals("zed.exe", StringComparison.OrdinalIgnoreCase))
            {
                relativePath = "Zed.exe";
            }

            var stagedPath = Path.Combine(stagingDirectory, relativePath);
            CopyFile(filePath, stagedPath);
            stagedFiles.Add(new StagedFile(stagedPath, relativePath));
        }

        var cliPath = ResolveArtifactPath(workspaceRoot, releaseDirectory, architecture, "cli.exe");
        if (File.Exists(cliPath))
        {
            var stagedPath = Path.Combine(stagingDirectory, "bin", "zed.exe");
            CopyFile(cliPath, stagedPath);
            stagedFiles.Add(new StagedFile(stagedPath, Path.Combine("bin", "zed.exe")));
        }

        var zedShPath = Path.Combine(workspaceRoot, "crates", "zed", "resources", "windows", "zed.sh");
        if (File.Exists(zedShPath))
        {
            var stagedPath = Path.Combine(stagingDirectory, "bin", "zed");
            CopyFile(zedShPath, stagedPath);
            stagedFiles.Add(new StagedFile(stagedPath, Path.Combine("bin", "zed")));
        }

        var zedCmdPath = Path.Combine(workspaceRoot, "crates", "zed", "resources", "windows", "zed.cmd");
        if (File.Exists(zedCmdPath))
        {
            var stagedPath = Path.Combine(stagingDirectory, "bin", "zed.cmd");
            CopyFile(zedCmdPath, stagedPath);
            stagedFiles.Add(new StagedFile(stagedPath, Path.Combine("bin", "zed.cmd")));
        }

        var iconPath = Path.Combine(workspaceRoot, "crates", "zed", "resources", "windows", channelConfig.AppIconName + ".ico");
        if (File.Exists(iconPath))
        {
            var stagedPath = Path.Combine(stagingDirectory, Path.GetFileName(iconPath));
            CopyFile(iconPath, stagedPath);
            stagedFiles.Add(new StagedFile(stagedPath, Path.GetFileName(iconPath)));
        }

        await StageConptyAsync(workspaceRoot, releaseDirectory, cacheDirectory, stagingDirectory, stagedFiles, architecture);

        if (architecture.Equals("x86_64", StringComparison.OrdinalIgnoreCase))
        {
            await StageAmdGpuServicesAsync(cacheDirectory, stagingDirectory, stagedFiles);
        }

        await StageExplorerShellFilesAsync(workspaceRoot, releaseDirectory, stagingDirectory, stagedFiles, channelConfig, architecture);

        return stagedFiles
            .GroupBy(file => file.RelativePath, StringComparer.OrdinalIgnoreCase)
            .Select(group => group.Last())
            .OrderBy(file => file.RelativePath, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    private static async Task StageConptyAsync(
        string workspaceRoot,
        string releaseDirectory,
        string cacheDirectory,
        string stagingDirectory,
        List<StagedFile> stagedFiles,
        string architecture)
    {
        var releaseConptyPath = Path.Combine(releaseDirectory, "conpty.dll");
        if (File.Exists(releaseConptyPath))
        {
            var releaseConptyDestinationPath = Path.Combine(stagingDirectory, "conpty.dll");
            CopyFile(releaseConptyPath, releaseConptyDestinationPath);
            stagedFiles.Add(new StagedFile(releaseConptyDestinationPath, "conpty.dll"));

            var releaseConsolePath = Path.Combine(releaseDirectory, "OpenConsole.exe");
            if (File.Exists(releaseConsolePath))
            {
                var releaseConsoleDestinationPath = Path.Combine(
                    stagingDirectory,
                    architecture.Equals("aarch64", StringComparison.OrdinalIgnoreCase) ? "arm64" : "x64",
                    "OpenConsole.exe");
                CopyFile(releaseConsolePath, releaseConsoleDestinationPath);
                stagedFiles.Add(new StagedFile(
                    releaseConsoleDestinationPath,
                    Path.Combine(architecture.Equals("aarch64", StringComparison.OrdinalIgnoreCase) ? "arm64" : "x64", "OpenConsole.exe")));
            }

            if (!architecture.Equals("aarch64", StringComparison.OrdinalIgnoreCase))
            {
                var fallbackConsolePath = Path.Combine(workspaceRoot, "crates", "zed", "resources", "windows", "bin", "x64", "OpenConsole.exe");
                if (File.Exists(fallbackConsolePath))
                {
                    var resourceConsoleDestinationPath = Path.Combine(stagingDirectory, "arm64", "OpenConsole.exe");
                    CopyFile(fallbackConsolePath, resourceConsoleDestinationPath);
                    stagedFiles.Add(new StagedFile(resourceConsoleDestinationPath, Path.Combine("arm64", "OpenConsole.exe")));
                }
            }

            return;
        }

        var archivePath = Path.Combine(cacheDirectory, ConptyArchiveName);
        var extractDirectory = Path.Combine(cacheDirectory, "conpty");
        var expectedDllPath = Path.Combine(extractDirectory, "runtimes", architecture.Equals("aarch64", StringComparison.OrdinalIgnoreCase) ? "win-arm64" : "win-x64", "native", "conpty.dll");
        if (!File.Exists(expectedDllPath))
        {
            await EnsureArchiveExtractedAsync(ConptyUrl, archivePath, extractDirectory);
        }

        var conptySourcePath = Path.Combine(extractDirectory, "runtimes", architecture.Equals("aarch64", StringComparison.OrdinalIgnoreCase) ? "win-arm64" : "win-x64", "native", "conpty.dll");
        if (!File.Exists(conptySourcePath))
        {
            throw new InvalidOperationException($"ConPTY runtime was not found after extraction: {conptySourcePath}");
        }

        var conptyDestinationPath = Path.Combine(stagingDirectory, "conpty.dll");
        CopyFile(conptySourcePath, conptyDestinationPath);
        stagedFiles.Add(new StagedFile(conptyDestinationPath, "conpty.dll"));

        if (architecture.Equals("aarch64", StringComparison.OrdinalIgnoreCase))
        {
            var arm64ConsoleSourcePath = Path.Combine(extractDirectory, "build", "native", "runtimes", "arm64", "OpenConsole.exe");
            var arm64ConsoleDestinationPath = Path.Combine(stagingDirectory, "arm64", "OpenConsole.exe");
            CopyFile(arm64ConsoleSourcePath, arm64ConsoleDestinationPath);
            stagedFiles.Add(new StagedFile(arm64ConsoleDestinationPath, Path.Combine("arm64", "OpenConsole.exe")));
            return;
        }

        var resourceConsolePath = Path.Combine(workspaceRoot, "crates", "zed", "resources", "windows", "bin", "x64", "OpenConsole.exe");
        var x64ConsoleSourcePath = File.Exists(resourceConsolePath)
            ? resourceConsolePath
            : Path.Combine(extractDirectory, "build", "native", "runtimes", "x64", "OpenConsole.exe");
        var x64ConsoleDestinationPath = Path.Combine(stagingDirectory, "x64", "OpenConsole.exe");
        CopyFile(x64ConsoleSourcePath, x64ConsoleDestinationPath);
        stagedFiles.Add(new StagedFile(x64ConsoleDestinationPath, Path.Combine("x64", "OpenConsole.exe")));

        var arm64ConsoleSourcePathFallback = Path.Combine(extractDirectory, "build", "native", "runtimes", "arm64", "OpenConsole.exe");
        if (File.Exists(arm64ConsoleSourcePathFallback))
        {
            var arm64ConsoleDestinationPath = Path.Combine(stagingDirectory, "arm64", "OpenConsole.exe");
            CopyFile(arm64ConsoleSourcePathFallback, arm64ConsoleDestinationPath);
            stagedFiles.Add(new StagedFile(arm64ConsoleDestinationPath, Path.Combine("arm64", "OpenConsole.exe")));
        }
    }

    private static async Task StageAmdGpuServicesAsync(
        string cacheDirectory,
        string stagingDirectory,
        List<StagedFile> stagedFiles)
    {
        var archivePath = Path.Combine(cacheDirectory, AgsArchiveName);
        var extractDirectory = Path.Combine(cacheDirectory, "ags");
        var expectedDllPath = Path.Combine(extractDirectory, "AGS_SDK-6.3.0", "ags_lib", "lib", "amd_ags_x64.dll");
        if (!File.Exists(expectedDllPath))
        {
            await EnsureArchiveExtractedAsync(AgsUrl, archivePath, extractDirectory);
        }

        if (File.Exists(expectedDllPath))
        {
            var destinationPath = Path.Combine(stagingDirectory, "amd_ags_x64.dll");
            CopyFile(expectedDllPath, destinationPath);
            stagedFiles.Add(new StagedFile(destinationPath, "amd_ags_x64.dll"));
        }
    }

    private static async Task StageExplorerShellFilesAsync(
        string workspaceRoot,
        string releaseDirectory,
        string stagingDirectory,
        List<StagedFile> stagedFiles,
        ChannelConfig channelConfig,
        string architecture)
    {
        var explorerDllPath = ResolveArtifactPath(workspaceRoot, releaseDirectory, architecture, "explorer_command_injector.dll");
        if (!File.Exists(explorerDllPath))
        {
            return;
        }

        var appxDirectory = Path.Combine(stagingDirectory, "appx");
        Directory.CreateDirectory(appxDirectory);

        var stagedExplorerDllPath = Path.Combine(appxDirectory, "zed_explorer_command_injector.dll");
        CopyFile(explorerDllPath, stagedExplorerDllPath);
        stagedFiles.Add(new StagedFile(stagedExplorerDllPath, Path.Combine("appx", "zed_explorer_command_injector.dll")));

        var manifestSourcePath = Path.Combine(
            workspaceRoot,
            "crates",
            "explorer_command_injector",
            channelConfig.Channel switch
            {
                "stable" => "AppxManifest.xml",
                "preview" => "AppxManifest-Preview.xml",
                _ => "AppxManifest-Nightly.xml",
            });

        if (!File.Exists(manifestSourcePath))
        {
            return;
        }

        var makeAppxPath = FindMakeAppxPath();
        if (makeAppxPath is null)
        {
            Console.WriteLine("makeAppx.exe was not found. Explorer AppX packaging will be skipped.");
            return;
        }

        var workingDirectory = Path.Combine(stagingDirectory, ".make_appx");
        RecreateDirectory(workingDirectory);
        CopyFile(manifestSourcePath, Path.Combine(workingDirectory, "AppxManifest.xml"));

        var appxPath = Path.Combine(appxDirectory, "zed_explorer_command_injector.appx");
        RunProcess(
            makeAppxPath,
            ["pack", "/d", workingDirectory, "/p", appxPath, "/nv"],
            stagingDirectory,
            "makeAppx packaging");
        stagedFiles.Add(new StagedFile(appxPath, Path.Combine("appx", "zed_explorer_command_injector.appx")));
    }

    private static async Task<string> DiscoverVersionAsync(string workspaceRoot)
    {
        var output = RunProcess(
            "cargo",
            ["metadata", "--format-version=1", "--no-deps", "--offline"],
            workspaceRoot,
            "cargo metadata",
            printOutput: false);

        using var document = JsonDocument.Parse(output);
        foreach (var package in document.RootElement.GetProperty("packages").EnumerateArray())
        {
            if (package.GetProperty("name").GetString() == "zed")
            {
                var version = package.GetProperty("version").GetString();
                if (!string.IsNullOrWhiteSpace(version))
                {
                    return version;
                }
            }
        }

        throw new InvalidOperationException("Unable to resolve the Zed crate version from cargo metadata.");
    }

    private static string DiscoverReleaseChannel(string workspaceRoot)
    {
        var releaseChannelPath = Path.Combine(workspaceRoot, "crates", "zed", "RELEASE_CHANNEL");
        if (!File.Exists(releaseChannelPath))
        {
            throw new InvalidOperationException($"Release channel file was not found: {releaseChannelPath}");
        }

        return File.ReadAllText(releaseChannelPath).Trim();
    }

    private static string ResolveReleaseDirectory(string workspaceRoot, string targetTriple)
    {
        var candidateDirectories = new[]
        {
            Path.Combine(workspaceRoot, "target", "release"),
            Path.Combine(workspaceRoot, "target", targetTriple, "release"),
        };

        foreach (var candidateDirectory in candidateDirectories)
        {
            if (File.Exists(Path.Combine(candidateDirectory, "zed.exe")) ||
                File.Exists(Path.Combine(candidateDirectory, "Zed.exe")))
            {
                return candidateDirectory;
            }
        }

        return candidateDirectories[0];
    }

    private static string ResolveArtifactPath(string workspaceRoot, string releaseDirectory, string architecture, string fileName)
    {
        var targetTriple = $"{architecture}-pc-windows-msvc";
        var candidatePaths = new[]
        {
            Path.Combine(releaseDirectory, fileName),
            Path.Combine(workspaceRoot, "target", targetTriple, "release", fileName),
            Path.Combine(workspaceRoot, "target", "release", fileName),
        };

        return candidatePaths.FirstOrDefault(File.Exists) ?? candidatePaths[0];
    }

    private static string ResolveWorkspaceRoot(string? explicitWorkspaceRoot)
    {
        if (!string.IsNullOrWhiteSpace(explicitWorkspaceRoot))
        {
            return Path.GetFullPath(explicitWorkspaceRoot);
        }

        var currentDirectory = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (currentDirectory is not null)
        {
            if (File.Exists(Path.Combine(currentDirectory.FullName, "Cargo.toml")))
            {
                return currentDirectory.FullName;
            }

            currentDirectory = currentDirectory.Parent;
        }

        throw new InvalidOperationException("Unable to locate the workspace root. Pass --workspace-root explicitly.");
    }

    private static string EnsureWixUiExtension(string workspaceRoot)
    {
        var extensionPath = FindWixUiExtensionPath();
        if (extensionPath is not null)
        {
            return extensionPath;
        }

        RunProcess(
            "wix",
            ["extension", "add", "-g", $"{WixUiExtensionId}/{WixUiExtensionVersion}"],
            workspaceRoot,
            "WiX UI extension installation",
            printOutput: false);

        extensionPath = FindWixUiExtensionPath();
        if (extensionPath is not null)
        {
            return extensionPath;
        }

        throw new InvalidOperationException("WiX UI extension was not found after installation.");
    }

    private static string? FindWixUiExtensionPath()
    {
        var userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (string.IsNullOrWhiteSpace(userProfile))
        {
            return null;
        }

        var extensionDirectory = Path.Combine(
            userProfile,
            ".wix",
            "extensions",
            WixUiExtensionId,
            WixUiExtensionVersion,
            "wixext7");
        if (!Directory.Exists(extensionDirectory))
        {
            return null;
        }

        return Directory
            .EnumerateFiles(extensionDirectory, "*.dll", SearchOption.TopDirectoryOnly)
            .FirstOrDefault(path => Path.GetFileName(path).Equals("WixToolset.UI.wixext.dll", StringComparison.OrdinalIgnoreCase));
    }

    private static string NormalizeMsiVersion(string version)
    {
        var coreVersion = version.Split('-', '+')[0];
        var parts = coreVersion.Split('.', StringSplitOptions.RemoveEmptyEntries);
        var numericParts = new[] { 0, 0, 0 };
        for (var index = 0; index < Math.Min(3, parts.Length); index++)
        {
            if (int.TryParse(parts[index], NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsedValue))
            {
                numericParts[index] = Math.Clamp(parsedValue, 0, 65535);
            }
        }

        return string.Join('.', numericParts);
    }

    private static List<string> ParseFileAssociations(string issPath)
    {
        var regex = new Regex(@"Software\\Classes\\(?<ext>\.[^\\]+)\\OpenWithProgids", RegexOptions.Compiled);

        return File.ReadLines(issPath)
            .Select(line => regex.Match(line))
            .Where(match => match.Success)
            .Select(match => match.Groups["ext"].Value)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .OrderBy(extension => extension, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    private static void GenerateWixSource(
        string wixSourcePath,
        IReadOnlyList<StagedFile> stagedFiles,
        ChannelConfig channelConfig,
        string packageVersion,
        IReadOnlyList<string> fileAssociations,
        bool includeDesktopShortcut,
        string architecture)
    {
        XNamespace ns = "http://wixtoolset.org/schemas/v4/wxs";
        var directoryNodes = BuildDirectoryTree(stagedFiles);
        var package = new XElement(
            ns + "Package",
            new XAttribute("Name", channelConfig.DisplayName),
            new XAttribute("Manufacturer", "Zed Industries"),
            new XAttribute("Version", packageVersion),
            new XAttribute("UpgradeCode", channelConfig.UpgradeCode));

        package.Add(new XElement(
            ns + "SummaryInformation",
            new XAttribute("Description", "!(loc.SummaryDescription)"),
            new XAttribute("Manufacturer", "Zed Industries")));
        package.Add(new XElement(
            ns + "MajorUpgrade",
            new XAttribute("DowngradeErrorMessage", "!(loc.DowngradeErrorMessage)")));
        package.Add(new XElement(ns + "MediaTemplate", new XAttribute("EmbedCab", "yes")));

        var iconFile = stagedFiles.FirstOrDefault(file => file.RelativePath.Equals(channelConfig.AppIconName + ".ico", StringComparison.OrdinalIgnoreCase));
        if (iconFile is not null)
        {
            package.Add(new XElement(
                ns + "Icon",
                new XAttribute("Id", "AppIcon"),
                new XAttribute("SourceFile", iconFile.SourcePath)));
            package.Add(new XElement(
                ns + "Property",
                new XAttribute("Id", "ARPPRODUCTICON"),
                new XAttribute("Value", "AppIcon")));
        }

        package.Add(new XElement(
            ns + "Property",
            new XAttribute("Id", "ARPURLINFOABOUT"),
            new XAttribute("Value", "https://www.zed.dev/")));
        package.Add(new XElement(
            ns + "Property",
            new XAttribute("Id", "ARPHELPLINK"),
            new XAttribute("Value", "https://www.zed.dev/")));
        package.Add(new XElement(
            ns + "Property",
            new XAttribute("Id", "WIXUI_INSTALLDIR"),
            new XAttribute("Value", "INSTALLFOLDER")));
        package.Add(new XElement(
            ns + "Property",
            new XAttribute("Id", "INSTALLFOLDER"),
            new XElement(
                ns + "RegistrySearch",
                new XAttribute("Id", "RememberedInstallDir"),
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", $"Software\\Zed Industries\\{channelConfig.RegValueName}"),
                new XAttribute("Name", "InstallDir"),
                new XAttribute("Type", "directory"))));
        package.Add(new XElement(
            ns + "Property",
            new XAttribute("Id", "DESKTOP_SHORTCUT"),
            new XAttribute("Value", includeDesktopShortcut ? "1" : "0"),
            new XElement(
                ns + "RegistrySearch",
                new XAttribute("Id", "RememberedDesktopShortcut"),
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", $"Software\\Zed Industries\\{channelConfig.RegValueName}"),
                new XAttribute("Name", "DesktopShortcut"),
                new XAttribute("Type", "raw"))));

        var feature = new XElement(
            ns + "Feature",
            new XAttribute("Id", "MainFeature"),
            new XAttribute("Title", channelConfig.DisplayName),
            new XAttribute("Description", "!(loc.MainFeatureDescription)"),
            new XAttribute("ConfigurableDirectory", "INSTALLFOLDER"),
            new XAttribute("Level", "1"));
        package.Add(feature);

        var wix = new XElement(
            ns + "Wix",
            package);

        wix.Add(CreateDirectoryFragment(ns, directoryNodes, channelConfig.DisplayName));

        var fileGroups = stagedFiles
            .GroupBy(file => NormalizeRelativeDirectory(Path.GetDirectoryName(file.RelativePath)))
            .OrderBy(group => group.Key, StringComparer.OrdinalIgnoreCase)
            .ToList();

        foreach (var fileGroup in fileGroups)
        {
            var componentGroupId = MakeId("CG", fileGroup.Key.Length == 0 ? "root" : fileGroup.Key);
            var directoryId = fileGroup.Key.Length == 0 ? "INSTALLFOLDER" : GetDirectoryId(fileGroup.Key);
            feature.Add(new XElement(ns + "ComponentGroupRef", new XAttribute("Id", componentGroupId)));

            var componentGroup = new XElement(
                ns + "ComponentGroup",
                new XAttribute("Id", componentGroupId),
                new XAttribute("Directory", directoryId));

            foreach (var stagedFile in fileGroup.OrderBy(file => file.RelativePath, StringComparer.OrdinalIgnoreCase))
            {
                var component = new XElement(ns + "Component", new XAttribute("Id", MakeId("CMP", stagedFile.RelativePath)));
                var fileElement = new XElement(
                    ns + "File",
                    new XAttribute("Id", MakeId("FIL", stagedFile.RelativePath)),
                    new XAttribute("Source", stagedFile.SourcePath));

                if (stagedFile.RelativePath.Equals("Zed.exe", StringComparison.OrdinalIgnoreCase))
                {
                    fileElement.Add(new XAttribute("KeyPath", "yes"));
                    component.Add(fileElement);
                    component.Add(new XElement(
                        ns + "Shortcut",
                        new XAttribute("Id", "StartMenuShortcut"),
                        new XAttribute("Directory", "PROGRAM_MENU_DIR"),
                        new XAttribute("Name", channelConfig.DisplayName),
                        new XAttribute("Target", "[INSTALLFOLDER]Zed.exe"),
                        new XAttribute("WorkingDirectory", "INSTALLFOLDER"),
                        new XAttribute("Description", channelConfig.DisplayName)));
                }
                else
                {
                    component.Add(fileElement);
                }

                componentGroup.Add(component);
            }

            wix.Add(new XElement(ns + "Fragment", componentGroup));
        }

        var integrationComponentGroupIds = new List<string>
        {
            AddPathIntegration(ns, wix, channelConfig),
            AddUriProtocol(ns, wix),
            AddShellIntegration(ns, wix, channelConfig),
            AddFileAssociations(ns, wix, channelConfig, fileAssociations),
        };

        foreach (var componentGroupId in integrationComponentGroupIds)
        {
            feature.Add(new XElement(ns + "ComponentGroupRef", new XAttribute("Id", componentGroupId)));
        }

        feature.Add(new XElement(ns + "ComponentGroupRef", new XAttribute("Id", "DesktopShortcutComponents")));
        feature.Add(new XElement(ns + "ComponentGroupRef", new XAttribute("Id", "InstallStateComponents")));
        wix.Add(CreateInstallerUiFragment(ns));
        wix.Add(CreateVerifyReadyDialogFragment(ns));
        wix.Add(CreateDesktopShortcutFragment(ns, channelConfig));
        wix.Add(CreateInstallStateFragment(ns, channelConfig));

        var document = new XDocument(new XDeclaration("1.0", "utf-8", "yes"), wix);
        Directory.CreateDirectory(Path.GetDirectoryName(wixSourcePath) ?? throw new InvalidOperationException("WiX source directory is invalid."));
        document.Save(wixSourcePath);
    }

    private static XElement CreateInstallerUiFragment(XNamespace ns)
    {
        return new XElement(
            ns + "Fragment",
            new XElement(
                ns + "UI",
                new XAttribute("Id", "ZedWixUI_InstallDir"),
                new XElement(ns + "TextStyle", new XAttribute("Id", "WixUI_Font_Normal"), new XAttribute("FaceName", "Tahoma"), new XAttribute("Size", "8")),
                new XElement(ns + "TextStyle", new XAttribute("Id", "WixUI_Font_Bigger"), new XAttribute("FaceName", "Tahoma"), new XAttribute("Size", "12")),
                new XElement(ns + "TextStyle", new XAttribute("Id", "WixUI_Font_Title"), new XAttribute("FaceName", "Tahoma"), new XAttribute("Size", "9"), new XAttribute("Bold", "yes")),
                new XElement(ns + "Property", new XAttribute("Id", "DefaultUIFont"), new XAttribute("Value", "WixUI_Font_Normal")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "BrowseDlg")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "DiskCostDlg")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "ErrorDlg")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "FatalError")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "FilesInUse")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "MsiRMFilesInUse")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "PrepareDlg")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "ProgressDlg")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "ResumeDlg")),
                new XElement(ns + "DialogRef", new XAttribute("Id", "UserExit")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "ExitDialog"), new XAttribute("Control", "Finish"), new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Order", "999")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "WelcomeDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "LicenseAgreementDlg"), new XAttribute("Condition", "NOT Installed")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "WelcomeDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "VerifyReadyDlg_WithDesktopShortcut"), new XAttribute("Condition", "Installed AND PATCH")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "LicenseAgreementDlg"), new XAttribute("Control", "Back"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "WelcomeDlg")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "LicenseAgreementDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "InstallDirDlg"), new XAttribute("Condition", "LicenseAccepted = \"1\"")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "InstallDirDlg"), new XAttribute("Control", "Back"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "LicenseAgreementDlg")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "InstallDirDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "CheckTargetPath"), new XAttribute("Value", "[WIXUI_INSTALLDIR]"), new XAttribute("Order", "1")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "InstallDirDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "SetTargetPath"), new XAttribute("Value", "[WIXUI_INSTALLDIR]"), new XAttribute("Order", "3")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "InstallDirDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "VerifyReadyDlg_WithDesktopShortcut"), new XAttribute("Order", "4")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "InstallDirDlg"), new XAttribute("Control", "ChangeFolder"), new XAttribute("Property", "_BrowseProperty"), new XAttribute("Value", "[WIXUI_INSTALLDIR]"), new XAttribute("Order", "1")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "InstallDirDlg"), new XAttribute("Control", "ChangeFolder"), new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "BrowseDlg"), new XAttribute("Order", "2")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "BrowseDlg"), new XAttribute("Control", "OK"), new XAttribute("Event", "CheckTargetPath"), new XAttribute("Value", "[WIXUI_INSTALLDIR]"), new XAttribute("Order", "1")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "BrowseDlg"), new XAttribute("Control", "OK"), new XAttribute("Event", "SetTargetPath"), new XAttribute("Value", "[_BrowseProperty]"), new XAttribute("Order", "3")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "BrowseDlg"), new XAttribute("Control", "OK"), new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Order", "4")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "VerifyReadyDlg_WithDesktopShortcut"), new XAttribute("Control", "Back"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "InstallDirDlg"), new XAttribute("Order", "1"), new XAttribute("Condition", "NOT Installed")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "VerifyReadyDlg_WithDesktopShortcut"), new XAttribute("Control", "Back"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "MaintenanceTypeDlg"), new XAttribute("Order", "2"), new XAttribute("Condition", "Installed AND NOT PATCH")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "VerifyReadyDlg_WithDesktopShortcut"), new XAttribute("Control", "Back"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "WelcomeDlg"), new XAttribute("Order", "2"), new XAttribute("Condition", "Installed AND PATCH")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "MaintenanceWelcomeDlg"), new XAttribute("Control", "Next"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "MaintenanceTypeDlg")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "MaintenanceTypeDlg"), new XAttribute("Control", "RepairButton"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "VerifyReadyDlg_WithDesktopShortcut")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "MaintenanceTypeDlg"), new XAttribute("Control", "RemoveButton"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "VerifyReadyDlg_WithDesktopShortcut")),
                new XElement(ns + "Publish", new XAttribute("Dialog", "MaintenanceTypeDlg"), new XAttribute("Control", "Back"), new XAttribute("Event", "NewDialog"), new XAttribute("Value", "MaintenanceWelcomeDlg"))),
            new XElement(ns + "UIRef", new XAttribute("Id", "WixUI_Common")));
    }

    private static XElement CreateVerifyReadyDialogFragment(XNamespace ns)
    {
        return new XElement(
            ns + "Fragment",
            new XElement(
                ns + "UI",
                new XElement(
                    ns + "Dialog",
                    new XAttribute("Id", "VerifyReadyDlg_WithDesktopShortcut"),
                    new XAttribute("Width", "370"),
                    new XAttribute("Height", "270"),
                    new XAttribute("Title", "!(loc.VerifyReadyDlg_Title)"),
                    new XAttribute("TrackDiskSpace", "yes"),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "Install"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("ElevationShield", "yes"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Default", "yes"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgInstall)"),
                        new XAttribute("ShowCondition", "NOT Installed AND ALLUSERS"),
                        new XAttribute("EnableCondition", "NOT Installed"),
                        new XAttribute("DefaultCondition", "NOT Installed"),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "InstallNoShield"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("ElevationShield", "no"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Default", "yes"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgInstall)"),
                        new XAttribute("ShowCondition", "NOT Installed AND NOT ALLUSERS"),
                        new XAttribute("EnableCondition", "NOT Installed"),
                        new XAttribute("DefaultCondition", "NOT Installed"),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "Repair"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Default", "yes"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgRepair)"),
                        new XAttribute("ShowCondition", "WixUI_InstallMode = \"Repair\""),
                        new XAttribute("EnableCondition", "WixUI_InstallMode = \"Repair\""),
                        new XAttribute("DefaultCondition", "WixUI_InstallMode = \"Repair\""),
                        new XElement(ns + "Publish", new XAttribute("Event", "ReinstallMode"), new XAttribute("Value", "ecmus"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "Reinstall"), new XAttribute("Value", "All"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "Update"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("ElevationShield", "yes"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgUpdate)"),
                        new XAttribute("ShowCondition", "WixUI_InstallMode = \"Update\" AND ALLUSERS"),
                        new XAttribute("EnableCondition", "WixUI_InstallMode = \"Update\""),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "UpdateNoShield"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("ElevationShield", "no"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgUpdate)"),
                        new XAttribute("ShowCondition", "WixUI_InstallMode = \"Update\" AND NOT ALLUSERS"),
                        new XAttribute("EnableCondition", "WixUI_InstallMode = \"Update\""),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "Remove"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("ElevationShield", "yes"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgRemove)"),
                        new XAttribute("ShowCondition", "WixUI_InstallMode = \"Remove\" AND ALLUSERS"),
                        new XAttribute("EnableCondition", "WixUI_InstallMode = \"Remove\""),
                        new XElement(ns + "Publish", new XAttribute("Event", "Remove"), new XAttribute("Value", "All"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(
                        ns + "Control",
                        new XAttribute("Id", "RemoveNoShield"),
                        new XAttribute("Type", "PushButton"),
                        new XAttribute("ElevationShield", "no"),
                        new XAttribute("X", "212"),
                        new XAttribute("Y", "243"),
                        new XAttribute("Width", "80"),
                        new XAttribute("Height", "17"),
                        new XAttribute("Hidden", "yes"),
                        new XAttribute("Disabled", "yes"),
                        new XAttribute("Text", "!(loc.VerifyReadyDlgRemove)"),
                        new XAttribute("ShowCondition", "WixUI_InstallMode = \"Remove\" AND NOT ALLUSERS"),
                        new XAttribute("EnableCondition", "WixUI_InstallMode = \"Remove\""),
                        new XElement(ns + "Publish", new XAttribute("Event", "Remove"), new XAttribute("Value", "All"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace <> 1")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfRbDiskDlg"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND (PROMPTROLLBACKCOST=\"P\" OR NOT PROMPTROLLBACKCOST)")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EndDialog"), new XAttribute("Value", "Return"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "EnableRollback"), new XAttribute("Value", "False"), new XAttribute("Condition", "OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 0 AND PROMPTROLLBACKCOST=\"D\"")),
                        new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "OutOfDiskDlg"), new XAttribute("Condition", "(OutOfDiskSpace = 1 AND OutOfNoRbDiskSpace = 1) OR (OutOfDiskSpace = 1 AND PROMPTROLLBACKCOST=\"F\")"))),
                    new XElement(ns + "Control", new XAttribute("Id", "InstallTitle"), new XAttribute("Type", "Text"), new XAttribute("X", "15"), new XAttribute("Y", "15"), new XAttribute("Width", "300"), new XAttribute("Height", "15"), new XAttribute("Transparent", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Hidden", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgInstallTitle)"), new XAttribute("ShowCondition", "NOT Installed")),
                    new XElement(ns + "Control", new XAttribute("Id", "InstallText"), new XAttribute("Type", "Text"), new XAttribute("X", "25"), new XAttribute("Y", "70"), new XAttribute("Width", "320"), new XAttribute("Height", "65"), new XAttribute("Hidden", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgInstallText)"), new XAttribute("ShowCondition", "NOT Installed")),
                    new XElement(ns + "Control", new XAttribute("Id", "RepairTitle"), new XAttribute("Type", "Text"), new XAttribute("X", "15"), new XAttribute("Y", "15"), new XAttribute("Width", "300"), new XAttribute("Height", "15"), new XAttribute("Transparent", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Hidden", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgRepairTitle)"), new XAttribute("ShowCondition", "WixUI_InstallMode = \"Repair\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "RepairText"), new XAttribute("Type", "Text"), new XAttribute("X", "25"), new XAttribute("Y", "70"), new XAttribute("Width", "320"), new XAttribute("Height", "80"), new XAttribute("Hidden", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgRepairText)"), new XAttribute("ShowCondition", "WixUI_InstallMode = \"Repair\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "UpdateTitle"), new XAttribute("Type", "Text"), new XAttribute("X", "15"), new XAttribute("Y", "15"), new XAttribute("Width", "300"), new XAttribute("Height", "15"), new XAttribute("Transparent", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Hidden", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgUpdateTitle)"), new XAttribute("ShowCondition", "WixUI_InstallMode = \"Update\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "UpdateText"), new XAttribute("Type", "Text"), new XAttribute("X", "25"), new XAttribute("Y", "70"), new XAttribute("Width", "320"), new XAttribute("Height", "80"), new XAttribute("Hidden", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgUpdateText)"), new XAttribute("ShowCondition", "WixUI_InstallMode = \"Update\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "RemoveTitle"), new XAttribute("Type", "Text"), new XAttribute("X", "15"), new XAttribute("Y", "15"), new XAttribute("Width", "300"), new XAttribute("Height", "15"), new XAttribute("Transparent", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Hidden", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgRemoveTitle)"), new XAttribute("ShowCondition", "WixUI_InstallMode = \"Remove\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "RemoveText"), new XAttribute("Type", "Text"), new XAttribute("X", "25"), new XAttribute("Y", "70"), new XAttribute("Width", "320"), new XAttribute("Height", "80"), new XAttribute("Hidden", "yes"), new XAttribute("NoPrefix", "yes"), new XAttribute("Text", "!(loc.VerifyReadyDlgRemoveText)"), new XAttribute("ShowCondition", "WixUI_InstallMode = \"Remove\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "DesktopShortcutCheckBox"), new XAttribute("Type", "CheckBox"), new XAttribute("X", "25"), new XAttribute("Y", "150"), new XAttribute("Width", "220"), new XAttribute("Height", "18"), new XAttribute("Hidden", "yes"), new XAttribute("Property", "DESKTOP_SHORTCUT"), new XAttribute("CheckBoxValue", "1"), new XAttribute("Text", "!(loc.DesktopShortcutCheckboxText)"), new XAttribute("ShowCondition", "NOT Installed")),
                    new XElement(ns + "Control", new XAttribute("Id", "Cancel"), new XAttribute("Type", "PushButton"), new XAttribute("X", "304"), new XAttribute("Y", "243"), new XAttribute("Width", "56"), new XAttribute("Height", "17"), new XAttribute("Cancel", "yes"), new XAttribute("Text", "!(loc.WixUICancel)"), new XElement(ns + "Publish", new XAttribute("Event", "SpawnDialog"), new XAttribute("Value", "CancelDlg"))),
                    new XElement(ns + "Control", new XAttribute("Id", "Back"), new XAttribute("Type", "PushButton"), new XAttribute("X", "156"), new XAttribute("Y", "243"), new XAttribute("Width", "56"), new XAttribute("Height", "17"), new XAttribute("Text", "!(loc.WixUIBack)"), new XAttribute("DefaultCondition", "WixUI_InstallMode = \"Remove\"")),
                    new XElement(ns + "Control", new XAttribute("Id", "BannerBitmap"), new XAttribute("Type", "Bitmap"), new XAttribute("X", "0"), new XAttribute("Y", "0"), new XAttribute("Width", "370"), new XAttribute("Height", "44"), new XAttribute("TabSkip", "no"), new XAttribute("Text", "!(loc.VerifyReadyDlgBannerBitmap)")),
                    new XElement(ns + "Control", new XAttribute("Id", "BannerLine"), new XAttribute("Type", "Line"), new XAttribute("X", "0"), new XAttribute("Y", "44"), new XAttribute("Width", "373"), new XAttribute("Height", "0")),
                    new XElement(ns + "Control", new XAttribute("Id", "BottomLine"), new XAttribute("Type", "Line"), new XAttribute("X", "0"), new XAttribute("Y", "234"), new XAttribute("Width", "373"), new XAttribute("Height", "0")))));
    }

    private static XElement CreateDirectoryFragment(XNamespace ns, DirectoryNode rootNode, string displayName)
    {
        var programFilesDirectory = new XElement(ns + "StandardDirectory", new XAttribute("Id", "ProgramFiles6432Folder"));
        var installDirectory = new XElement(
            ns + "Directory",
            new XAttribute("Id", "INSTALLFOLDER"),
            new XAttribute("Name", displayName));

        foreach (var child in rootNode.Children.OrderBy(child => child.Name, StringComparer.OrdinalIgnoreCase))
        {
            installDirectory.Add(CreateDirectoryElement(ns, child));
        }

        programFilesDirectory.Add(installDirectory);

        var programMenuDirectory = new XElement(
            ns + "StandardDirectory",
            new XAttribute("Id", "ProgramMenuFolder"),
            new XElement(
                ns + "Directory",
                new XAttribute("Id", "PROGRAM_MENU_DIR"),
                new XAttribute("Name", displayName)));

        return new XElement(ns + "Fragment", programFilesDirectory, programMenuDirectory);
    }

    private static XElement CreateDirectoryElement(XNamespace ns, DirectoryNode node)
    {
        var element = new XElement(
            ns + "Directory",
            new XAttribute("Id", node.Id),
            new XAttribute("Name", node.Name));

        foreach (var child in node.Children.OrderBy(child => child.Name, StringComparer.OrdinalIgnoreCase))
        {
            element.Add(CreateDirectoryElement(ns, child));
        }

        return element;
    }

    private static XElement CreateDesktopShortcutFragment(XNamespace ns, ChannelConfig channelConfig)
    {
        return new XElement(
            ns + "Fragment",
            new XElement(
                ns + "ComponentGroup",
                new XAttribute("Id", "DesktopShortcutComponents"),
                new XAttribute("Directory", "INSTALLFOLDER"),
                    new XElement(
                        ns + "Component",
                        new XAttribute("Id", "DesktopShortcutComponent"),
                        new XAttribute("Condition", "DESKTOP_SHORTCUT = \"1\""),
                    new XElement(
                        ns + "Shortcut",
                        new XAttribute("Id", "DesktopShortcut"),
                        new XAttribute("Directory", "DesktopFolder"),
                        new XAttribute("Name", channelConfig.DisplayName),
                        new XAttribute("Target", "[INSTALLFOLDER]Zed.exe"),
                        new XAttribute("WorkingDirectory", "INSTALLFOLDER"),
                        new XAttribute("Description", channelConfig.DisplayName)),
                    new XElement(
                        ns + "RegistryValue",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Zed Industries\\{channelConfig.RegValueName}"),
                        new XAttribute("Name", "DesktopShortcutInstalled"),
                        new XAttribute("Type", "string"),
                        new XAttribute("Value", "1"),
                        new XAttribute("KeyPath", "yes")))));
    }

    private static XElement CreateInstallStateFragment(XNamespace ns, ChannelConfig channelConfig)
    {
        return new XElement(
            ns + "Fragment",
            new XElement(
                ns + "ComponentGroup",
                new XAttribute("Id", "InstallStateComponents"),
                new XAttribute("Directory", "INSTALLFOLDER"),
                new XElement(
                    ns + "Component",
                    new XAttribute("Id", "InstallStateComponent"),
                    new XElement(
                        ns + "RegistryValue",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Zed Industries\\{channelConfig.RegValueName}"),
                        new XAttribute("Name", "InstallDir"),
                        new XAttribute("Type", "string"),
                        new XAttribute("Value", "[INSTALLFOLDER]"),
                        new XAttribute("KeyPath", "yes")),
                    new XElement(
                        ns + "RegistryValue",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Zed Industries\\{channelConfig.RegValueName}"),
                        new XAttribute("Name", "DesktopShortcut"),
                        new XAttribute("Type", "string"),
                        new XAttribute("Value", "[DESKTOP_SHORTCUT]")))));
    }

    private static IReadOnlyList<string> GenerateLocalizationFiles(string generatedDirectory, ChannelConfig channelConfig, string architecture)
    {
        return GenerateLocalizationFiles(generatedDirectory, channelConfig, architecture, SupportedLanguages);
    }

    private static IReadOnlyList<string> GenerateLocalizationFiles(
        string generatedDirectory,
        ChannelConfig channelConfig,
        string architecture,
        IReadOnlyList<string> languages)
    {
        var localizationFilePaths = new List<string>();

        foreach (var language in languages)
        {
            var localizationPath = Path.Combine(generatedDirectory, $"Zed.{language}.wxl");
            switch (language)
            {
                case "en-US":
                    WriteLocalizationFile(
                        localizationPath,
                        "en-US",
                        "1252",
                        channelConfig,
                        $"{channelConfig.DisplayName} for Windows ({architecture})",
                        $"A newer version of {channelConfig.DisplayName} is already installed.",
                        $"Install {channelConfig.DisplayName}",
                        "Create a desktop shortcut");
                    break;
                case "zh-CN":
                    WriteLocalizationFile(
                        localizationPath,
                        "zh-CN",
                        "936",
                        channelConfig,
                        $"{channelConfig.DisplayName} Windows 版 ({architecture})",
                        $"已安装更新版本的 {channelConfig.DisplayName}。",
                        $"安装 {channelConfig.DisplayName}",
                        "创建桌面快捷方式");
                    break;
                default:
                    throw new InvalidOperationException($"Unsupported localization language: {language}");
            }

            localizationFilePaths.Add(localizationPath);
        }

        return localizationFilePaths;
    }

    private static IReadOnlyList<string> ResolveLanguages(IReadOnlyList<string> requestedLanguages)
    {
        if (requestedLanguages.Count == 0)
        {
            return SupportedLanguages;
        }

        var languages = new List<string>();
        foreach (var requestedLanguage in requestedLanguages)
        {
            var normalizedLanguage = SupportedLanguages.FirstOrDefault(language =>
                language.Equals(requestedLanguage, StringComparison.OrdinalIgnoreCase));
            if (normalizedLanguage is null)
            {
                throw new InvalidOperationException($"Unsupported language: {requestedLanguage}. Supported languages: {string.Join(", ", SupportedLanguages)}");
            }

            if (!languages.Contains(normalizedLanguage, StringComparer.OrdinalIgnoreCase))
            {
                languages.Add(normalizedLanguage);
            }
        }

        return languages;
    }

    private static void WriteLocalizationFile(
        string localizationPath,
        string culture,
        string codepage,
        ChannelConfig channelConfig,
        string summaryDescription,
        string downgradeErrorMessage,
        string mainFeatureDescription,
        string desktopShortcutCheckboxText)
    {
        XNamespace localizationNamespace = "http://wixtoolset.org/schemas/v4/wxl";
        var document = new XDocument(
            new XDeclaration("1.0", "utf-8", "yes"),
            new XElement(
                localizationNamespace + "WixLocalization",
                new XAttribute("Culture", culture),
                new XAttribute("Codepage", codepage),
                culture.Equals("en-US", StringComparison.OrdinalIgnoreCase)
                    ? new XAttribute("ExtensionDefaultCulture", "yes")
                    : null,
                new XElement(localizationNamespace + "String", new XAttribute("Id", "ProductName"), new XAttribute("Value", channelConfig.DisplayName)),
                new XElement(localizationNamespace + "String", new XAttribute("Id", "SummaryDescription"), new XAttribute("Value", summaryDescription)),
                new XElement(localizationNamespace + "String", new XAttribute("Id", "DowngradeErrorMessage"), new XAttribute("Value", downgradeErrorMessage)),
                new XElement(localizationNamespace + "String", new XAttribute("Id", "MainFeatureDescription"), new XAttribute("Value", mainFeatureDescription)),
                new XElement(localizationNamespace + "String", new XAttribute("Id", "DesktopShortcutCheckboxText"), new XAttribute("Value", desktopShortcutCheckboxText))));
        document.Save(localizationPath);
    }

    private static string AddPathIntegration(XNamespace ns, XElement wix, ChannelConfig channelConfig)
    {
        var componentGroupId = "PathComponents";
        var component = new XElement(
            ns + "Component",
            new XAttribute("Id", "PathComponent"),
            new XElement(
                ns + "Environment",
                new XAttribute("Id", "AddToPath"),
                new XAttribute("Name", "PATH"),
                new XAttribute("Action", "set"),
                new XAttribute("Part", "last"),
                new XAttribute("Permanent", "no"),
                new XAttribute("System", "no"),
                new XAttribute("Value", "[INSTALLFOLDER]bin")),
            new XElement(
                ns + "RegistryValue",
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", $"Software\\Zed Industries\\{channelConfig.RegValueName}"),
                new XAttribute("Name", "AddToPath"),
                new XAttribute("Type", "integer"),
                new XAttribute("Value", "1"),
                new XAttribute("KeyPath", "yes")));

        wix.Add(new XElement(
            ns + "Fragment",
            new XElement(
                ns + "ComponentGroup",
                new XAttribute("Id", componentGroupId),
                new XAttribute("Directory", "INSTALLFOLDER"),
                component)));
        return componentGroupId;
    }

    private static string AddUriProtocol(XNamespace ns, XElement wix)
    {
        var componentGroupId = "UriProtocolComponents";
        var component = new XElement(
            ns + "Component",
            new XAttribute("Id", "UriProtocolComponent"),
            new XElement(
                ns + "RegistryKey",
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", "Software\\Classes\\zed"),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", "URL:zed Protocol"),
                    new XAttribute("KeyPath", "yes")),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Name", "URL Protocol"),
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", string.Empty))),
            new XElement(
                ns + "RegistryKey",
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", "Software\\Classes\\zed\\DefaultIcon"),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", "[INSTALLFOLDER]Zed.exe,1"))),
            new XElement(
                ns + "RegistryKey",
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", "Software\\Classes\\zed\\shell\\open\\command"),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", "\"[INSTALLFOLDER]Zed.exe\" \"%1\""))));

        wix.Add(new XElement(
            ns + "Fragment",
            new XElement(
                ns + "ComponentGroup",
                new XAttribute("Id", componentGroupId),
                new XAttribute("Directory", "INSTALLFOLDER"),
                component)));
        return componentGroupId;
    }

    private static string AddShellIntegration(XNamespace ns, XElement wix, ChannelConfig channelConfig)
    {
        var componentGroupId = "ShellIntegrationComponents";
        var group = new XElement(
            ns + "ComponentGroup",
            new XAttribute("Id", componentGroupId),
            new XAttribute("Directory", "INSTALLFOLDER"));

        var fileCommand = "\"[INSTALLFOLDER]Zed.exe\" \"%1\"";
        var folderCommand = "\"[INSTALLFOLDER]Zed.exe\" \"%V\"";

        group.Add(CreateShellComponent(ns, "ShellFilesComponent", $"Software\\Classes\\*\\shell\\{channelConfig.RegValueName}", channelConfig.ShellNameShort, fileCommand));
        group.Add(CreateShellComponent(ns, "ShellFoldersComponent", $"Software\\Classes\\directory\\shell\\{channelConfig.RegValueName}", channelConfig.ShellNameShort, folderCommand));
        group.Add(CreateShellComponent(ns, "ShellBackgroundComponent", $"Software\\Classes\\directory\\background\\shell\\{channelConfig.RegValueName}", channelConfig.ShellNameShort, folderCommand));
        group.Add(CreateShellComponent(ns, "ShellDriveComponent", $"Software\\Classes\\Drive\\shell\\{channelConfig.RegValueName}", channelConfig.ShellNameShort, folderCommand));

        wix.Add(new XElement(ns + "Fragment", group));
        return componentGroupId;
    }

    private static XElement CreateShellComponent(XNamespace ns, string componentId, string keyPath, string shellName, string command)
    {
        return new XElement(
            ns + "Component",
            new XAttribute("Id", componentId),
            new XElement(
                ns + "RegistryKey",
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", keyPath),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", $"Open with {shellName.Replace("&", string.Empty)}"),
                    new XAttribute("KeyPath", "yes")),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Name", "Icon"),
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", "[INSTALLFOLDER]Zed.exe"))),
            new XElement(
                ns + "RegistryKey",
                new XAttribute("Root", "HKCU"),
                new XAttribute("Key", keyPath + "\\command"),
                new XElement(
                    ns + "RegistryValue",
                    new XAttribute("Type", "string"),
                    new XAttribute("Value", command))));
    }

    private static string AddFileAssociations(XNamespace ns, XElement wix, ChannelConfig channelConfig, IReadOnlyList<string> fileAssociations)
    {
        var componentGroupId = "FileAssociationComponents";
        var group = new XElement(
            ns + "ComponentGroup",
            new XAttribute("Id", componentGroupId),
            new XAttribute("Directory", "INSTALLFOLDER"));

        foreach (var extension in fileAssociations)
        {
            var progId = channelConfig.RegValueName + extension;
            var extensionLabel = extension.TrimStart('.').Replace('_', ' ');

            group.Add(
                new XElement(
                    ns + "Component",
                    new XAttribute("Id", MakeId("ASSOC", extension)),
                    new XElement(
                        ns + "RegistryKey",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Classes\\{extension}\\OpenWithProgids"),
                        new XElement(
                            ns + "RegistryValue",
                            new XAttribute("Name", progId),
                            new XAttribute("Type", "string"),
                            new XAttribute("Value", string.Empty),
                            new XAttribute("KeyPath", "yes"))),
                    new XElement(
                        ns + "RegistryKey",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Classes\\{progId}"),
                        new XElement(
                            ns + "RegistryValue",
                            new XAttribute("Type", "string"),
                            new XAttribute("Value", $"{channelConfig.DisplayName} {extensionLabel} file")),
                        new XElement(
                            ns + "RegistryValue",
                            new XAttribute("Name", "AppUserModelID"),
                            new XAttribute("Type", "string"),
                            new XAttribute("Value", channelConfig.AppUserId))),
                    new XElement(
                        ns + "RegistryKey",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Classes\\{progId}\\DefaultIcon"),
                        new XElement(
                            ns + "RegistryValue",
                            new XAttribute("Type", "string"),
                            new XAttribute("Value", "[INSTALLFOLDER]Zed.exe"))),
                    new XElement(
                        ns + "RegistryKey",
                        new XAttribute("Root", "HKCU"),
                        new XAttribute("Key", $"Software\\Classes\\{progId}\\shell\\open\\command"),
                        new XElement(
                            ns + "RegistryValue",
                            new XAttribute("Type", "string"),
                            new XAttribute("Value", "\"[INSTALLFOLDER]Zed.exe\" \"%1\"")))));
        }

        wix.Add(new XElement(ns + "Fragment", group));
        return componentGroupId;
    }

    private static DirectoryNode BuildDirectoryTree(IReadOnlyList<StagedFile> stagedFiles)
    {
        var root = new DirectoryNode("INSTALLFOLDER", string.Empty, string.Empty);
        foreach (var stagedFile in stagedFiles)
        {
            var relativeDirectory = NormalizeRelativeDirectory(Path.GetDirectoryName(stagedFile.RelativePath));
            if (relativeDirectory.Length == 0)
            {
                continue;
            }

            var segments = relativeDirectory.Split(Path.DirectorySeparatorChar, StringSplitOptions.RemoveEmptyEntries);
            var currentNode = root;
            var currentPath = new StringBuilder();
            foreach (var segment in segments)
            {
                if (currentPath.Length > 0)
                {
                    currentPath.Append(Path.DirectorySeparatorChar);
                }

                currentPath.Append(segment);
                var pathValue = currentPath.ToString();
                var childNode = currentNode.Children.FirstOrDefault(child => child.Path.Equals(pathValue, StringComparison.OrdinalIgnoreCase));
                if (childNode is null)
                {
                    childNode = new DirectoryNode(GetDirectoryId(pathValue), segment, pathValue);
                    currentNode.Children.Add(childNode);
                }

                currentNode = childNode;
            }
        }

        return root;
    }

    private static string GetDirectoryId(string relativeDirectory)
    {
        return MakeId("DIR", relativeDirectory.Replace(Path.DirectorySeparatorChar, '_').Replace(Path.AltDirectorySeparatorChar, '_'));
    }

    private static string NormalizeRelativeDirectory(string? relativeDirectory)
    {
        if (string.IsNullOrWhiteSpace(relativeDirectory))
        {
            return string.Empty;
        }

        return relativeDirectory.Replace(Path.AltDirectorySeparatorChar, Path.DirectorySeparatorChar).Trim('.', Path.DirectorySeparatorChar);
    }

    private static string MakeId(string prefix, string value)
    {
        var sanitizedValue = Regex.Replace(value, "[^A-Za-z0-9_]", "_");
        if (sanitizedValue.Length > 40)
        {
            sanitizedValue = sanitizedValue[..40];
        }

        var hashBytes = SHA256.HashData(Encoding.UTF8.GetBytes(value));
        var hash = Convert.ToHexString(hashBytes)[..10];
        return $"{prefix}_{sanitizedValue}_{hash}";
    }

    private static string? FindMakeAppxPath()
    {
        var windowsKitsRoot = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);
        if (!string.IsNullOrWhiteSpace(windowsKitsRoot))
        {
            var binRoot = Path.Combine(windowsKitsRoot, "Windows Kits", "10", "bin");
            if (Directory.Exists(binRoot))
            {
                foreach (var sdkDirectory in Directory.EnumerateDirectories(binRoot).OrderByDescending(path => path, StringComparer.OrdinalIgnoreCase))
                {
                    var candidatePath = Path.Combine(sdkDirectory, "x64", "makeAppx.exe");
                    if (File.Exists(candidatePath))
                    {
                        return candidatePath;
                    }
                }
            }
        }

        try
        {
            var output = RunProcess("where.exe", ["makeAppx.exe"], Directory.GetCurrentDirectory(), "where makeAppx.exe", throwOnFailure: false);
            return output.Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries)
                .FirstOrDefault(path => File.Exists(path));
        }
        catch
        {
            return null;
        }
    }

    private static async Task EnsureArchiveExtractedAsync(string url, string archivePath, string extractDirectory)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(archivePath) ?? throw new InvalidOperationException("Archive directory is invalid."));
        if (!File.Exists(archivePath))
        {
            Console.WriteLine($"Downloading {Path.GetFileName(archivePath)}");
            await using var outputStream = File.Create(archivePath);
            await using var inputStream = await HttpClient.GetStreamAsync(url);
            await inputStream.CopyToAsync(outputStream);
        }

        if (Directory.Exists(extractDirectory))
        {
            Directory.Delete(extractDirectory, recursive: true);
        }

        ZipFile.ExtractToDirectory(archivePath, extractDirectory);
    }

    private static string RunProcess(string fileName, IReadOnlyList<string> arguments, string workingDirectory, string description, bool throwOnFailure = true, bool printOutput = true)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = fileName,
            WorkingDirectory = workingDirectory,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };

        foreach (var argument in arguments)
        {
            startInfo.ArgumentList.Add(argument);
        }

        using var process = Process.Start(startInfo) ?? throw new InvalidOperationException($"Unable to start {description}.");
        var standardOutput = process.StandardOutput.ReadToEnd();
        var standardError = process.StandardError.ReadToEnd();
        process.WaitForExit();

        if (printOutput && !string.IsNullOrWhiteSpace(standardOutput))
        {
            Console.WriteLine(standardOutput.TrimEnd());
        }

        if (printOutput && !string.IsNullOrWhiteSpace(standardError))
        {
            Console.Error.WriteLine(standardError.TrimEnd());
        }

        if (process.ExitCode != 0 && throwOnFailure)
        {
            throw new InvalidOperationException($"{description} failed with exit code {process.ExitCode}.");
        }

        return standardOutput;
    }

    private static void CopyFile(string sourcePath, string destinationPath)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(destinationPath) ?? throw new InvalidOperationException("Destination directory is invalid."));
        File.Copy(sourcePath, destinationPath, overwrite: true);
    }

    private static void RecreateDirectory(string path)
    {
        if (Directory.Exists(path))
        {
            Directory.Delete(path, recursive: true);
        }

        Directory.CreateDirectory(path);
    }
}

internal sealed record StagedFile(string SourcePath, string RelativePath);

internal sealed class DirectoryNode
{
    public DirectoryNode(string id, string name, string path)
    {
        Id = id;
        Name = name;
        Path = path;
    }

    public string Id { get; }

    public string Name { get; }

    public string Path { get; }

    public List<DirectoryNode> Children { get; } = [];
}

internal sealed class ChannelConfig
{
    private ChannelConfig(
        string channel,
        string appIconName,
        string displayName,
        string regValueName,
        string appUserId,
        string shellNameShort,
        string upgradeCode,
        string outputBaseName)
    {
        Channel = channel;
        AppIconName = appIconName;
        DisplayName = displayName;
        RegValueName = regValueName;
        AppUserId = appUserId;
        ShellNameShort = shellNameShort;
        UpgradeCode = upgradeCode;
        OutputBaseName = outputBaseName;
    }

    public string Channel { get; }

    public string AppIconName { get; }

    public string DisplayName { get; }

    public string RegValueName { get; }

    public string AppUserId { get; }

    public string ShellNameShort { get; }

    public string UpgradeCode { get; }

    public string OutputBaseName { get; }

    public static ChannelConfig For(string channel)
    {
        return channel.ToLowerInvariant() switch
        {
            "stable" => new ChannelConfig(
                "stable",
                "app-icon",
                "Zed",
                "Zed",
                "ZedIndustries.Zed",
                "Z&ed",
                "2DB0DA96-CA55-49BB-AF4F-64AF36A86712",
                "Zed"),
            "preview" => new ChannelConfig(
                "preview",
                "app-icon-preview",
                "Zed Preview",
                "ZedPreview",
                "ZedIndustries.Zed.Preview",
                "Z&ed Preview",
                "F70E4811-D0E2-4D88-AC99-D63752799F95",
                "ZedPreview"),
            "nightly" => new ChannelConfig(
                "nightly",
                "app-icon-nightly",
                "Zed Nightly",
                "ZedNightly",
                "ZedIndustries.Zed.Nightly",
                "Z&ed Editor Nightly",
                "1BDB21D3-14E7-433C-843C-9C97382B2FE0",
                "ZedNightly"),
            "dev" => new ChannelConfig(
                "dev",
                "app-icon-dev",
                "Zed Dev",
                "ZedDev",
                "ZedIndustries.Zed.Dev",
                "Z&ed Dev",
                "8357632E-24A4-4F32-BA97-E575B4D1FE5D",
                "ZedDev"),
            _ => throw new InvalidOperationException($"Unsupported release channel: {channel}"),
        };
    }
}

internal sealed record Options
{
    public string? Architecture { get; private init; }

    public string? WorkspaceRoot { get; private init; }

    public string? ReleaseDirectory { get; private init; }

    public string? StagingDirectory { get; private init; }

    public string? OutputPath { get; private init; }

    public string? Channel { get; private init; }

    public string? Version { get; private init; }

    public IReadOnlyList<string> Languages { get; private init; } = [];

    public bool IncludeDesktopShortcut { get; private init; }

    public static Options Parse(IReadOnlyList<string> args)
    {
        var options = new Options();

        for (var index = 0; index < args.Count; index++)
        {
            switch (args[index])
            {
                case "--arch":
                case "-a":
                    options = options with { Architecture = ReadValue(args, ref index) };
                    break;
                case "--workspace-root":
                    options = options with { WorkspaceRoot = ReadValue(args, ref index) };
                    break;
                case "--release-dir":
                    options = options with { ReleaseDirectory = ReadValue(args, ref index) };
                    break;
                case "--staging-dir":
                    options = options with { StagingDirectory = ReadValue(args, ref index) };
                    break;
                case "--output":
                case "-o":
                    options = options with { OutputPath = ReadValue(args, ref index) };
                    break;
                case "--channel":
                    options = options with { Channel = ReadValue(args, ref index) };
                    break;
                case "--version":
                    options = options with { Version = ReadValue(args, ref index) };
                    break;
                case "--desktop-shortcut":
                    options = options with { IncludeDesktopShortcut = true };
                    break;
                case "--language":
                    options = options with { Languages = [.. options.Languages, ReadValue(args, ref index)] };
                    break;
                case "--help":
                case "-h":
                    PrintUsage();
                    Environment.Exit(0);
                    break;
                default:
                    throw new InvalidOperationException($"Unknown argument: {args[index]}");
            }
        }

        return options;
    }

    private static string ReadValue(IReadOnlyList<string> args, ref int index)
    {
        if (index + 1 >= args.Count)
        {
            throw new InvalidOperationException($"Missing value for argument: {args[index]}");
        }

        index += 1;
        return args[index];
    }

    private static void PrintUsage()
    {
        Console.WriteLine("Usage: WindowsMsiBuilder [options]");
        Console.WriteLine("  --arch, -a            Target architecture (x86_64 or aarch64)");
        Console.WriteLine("  --workspace-root      Path to the workspace root");
        Console.WriteLine("  --release-dir         Existing target/<triple>/release directory");
        Console.WriteLine("  --staging-dir         Directory used for MSI staging");
        Console.WriteLine("  --output, -o          Output MSI path");
        Console.WriteLine("  --channel             Release channel override");
        Console.WriteLine("  --version             Product version override");
        Console.WriteLine("  --language            MSI UI language to include (repeatable: en-US, zh-CN)");
        Console.WriteLine("  --desktop-shortcut    Add a desktop shortcut in the MSI");
    }
}
