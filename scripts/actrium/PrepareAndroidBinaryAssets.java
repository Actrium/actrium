import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.UUID;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

public final class PrepareAndroidBinaryAssets {
    private static final String MARKER_FILE = ".actrium-binary-assets.lock";

    private record Artifact(
            String id,
            String component,
            URI url,
            String archiveName,
            String sha256,
            int stripComponents,
            Path destination,
            List<Path> requiredPaths) {}

    private record Options(
            Path lockFile,
            String component,
            Path outputDirectory,
            Path cacheDirectory,
            boolean offline,
            boolean validateOnly) {}

    public static void main(String[] args) throws Exception {
        Options options = parseOptions(args);
        Properties lock = new Properties();
        try (InputStream input = Files.newInputStream(options.lockFile())) {
            lock.load(input);
        }

        if (!"1".equals(lock.getProperty("schemaVersion"))) {
            throw new IllegalArgumentException("unsupported binary asset lock schema");
        }

        List<Artifact> allArtifacts = readArtifacts(lock);
        if (options.validateOnly()) {
            System.out.println(
                    "Android binary asset lock is valid: "
                            + allArtifacts.size()
                            + " artifact(s)");
            return;
        }

        List<Artifact> artifacts = allArtifacts.stream()
                .filter(artifact -> artifact.component().equals(options.component()))
                .toList();
        if (artifacts.isEmpty()) {
            throw new IllegalArgumentException(
                    "no artifacts found for component " + options.component());
        }

        String signature = sha256(options.lockFile()) + ":" + options.component();
        if (isPrepared(options.outputDirectory(), signature, artifacts)) {
            System.out.println(
                    "Android binary assets already prepared: " + options.component());
            return;
        }

        Files.createDirectories(options.cacheDirectory());
        List<Path> archives = new ArrayList<>();
        for (Artifact artifact : artifacts) {
            archives.add(resolveArchive(artifact, options.cacheDirectory(), options.offline()));
        }

        Path output = options.outputDirectory();
        Path parent = output.getParent();
        if (parent == null) {
            throw new IllegalArgumentException("output directory must not be a filesystem root");
        }
        Files.createDirectories(parent);
        Path staging = parent.resolve(
                "." + output.getFileName() + ".tmp-" + UUID.randomUUID()).normalize();
        if (!staging.startsWith(parent)) {
            throw new IllegalArgumentException("invalid staging directory");
        }

        try {
            Files.createDirectories(staging);
            for (int index = 0; index < artifacts.size(); index++) {
                extractArchive(artifacts.get(index), archives.get(index), staging);
            }
            verifyRequiredPaths(staging, artifacts);
            Files.writeString(
                    staging.resolve(MARKER_FILE),
                    signature + System.lineSeparator(),
                    StandardCharsets.UTF_8);
            replaceGeneratedDirectory(output, staging);
        } catch (Exception error) {
            deleteRecursively(staging);
            throw error;
        }

        System.out.println("Prepared Android binary assets: " + options.component());
    }

    private static Options parseOptions(String[] args) {
        Map<String, String> values = new LinkedHashMap<>();
        boolean offline = false;
        boolean validateOnly = false;
        for (int index = 0; index < args.length; index++) {
            String argument = args[index];
            if ("--offline".equals(argument)) {
                offline = true;
                continue;
            }
            if ("--validate-only".equals(argument)) {
                validateOnly = true;
                continue;
            }
            if (!argument.startsWith("--") || index + 1 >= args.length) {
                throw new IllegalArgumentException("invalid argument: " + argument);
            }
            values.put(argument, args[++index]);
        }

        Path lockFile = requiredPath(values, "--lock");
        if (!Files.isRegularFile(lockFile)) {
            throw new IllegalArgumentException("lock file does not exist: " + lockFile);
        }
        if (validateOnly) {
            return new Options(lockFile, "", null, null, offline, true);
        }

        String component = requiredValue(values, "--component");
        Path output = requiredPath(values, "--output");
        Path cache = requiredPath(values, "--cache");
        if (output.getParent() == null || cache.getParent() == null) {
            throw new IllegalArgumentException("output and cache must not be filesystem roots");
        }
        return new Options(lockFile, component, output, cache, offline, false);
    }

    private static Path requiredPath(Map<String, String> values, String name) {
        return Path.of(requiredValue(values, name)).toAbsolutePath().normalize();
    }

    private static String requiredValue(Map<String, String> values, String name) {
        String value = values.get(name);
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("missing required argument " + name);
        }
        return value;
    }

    private static List<Artifact> readArtifacts(Properties lock) {
        String rawIds = lock.getProperty("artifacts", "");
        if (rawIds.isBlank()) {
            throw new IllegalArgumentException("binary asset lock has no artifacts");
        }

        List<Artifact> artifacts = new ArrayList<>();
        for (String rawId : rawIds.split(",")) {
            String id = rawId.trim();
            if (!id.matches("[a-z0-9-]+")) {
                throw new IllegalArgumentException("invalid artifact id: " + id);
            }
            String prefix = id + ".";
            String component = requiredProperty(lock, prefix + "component");
            URI url = URI.create(requiredProperty(lock, prefix + "url"));
            if (!"https".equalsIgnoreCase(url.getScheme())) {
                throw new IllegalArgumentException("artifact URL must use HTTPS: " + id);
            }
            String archiveName = requiredProperty(lock, prefix + "archiveName");
            if (!archiveName.matches("[A-Za-z0-9._-]+")) {
                throw new IllegalArgumentException("invalid archive name: " + archiveName);
            }
            String checksum = requiredProperty(lock, prefix + "sha256").toLowerCase();
            if (!checksum.matches("[0-9a-f]{64}")) {
                throw new IllegalArgumentException("invalid SHA-256 for " + id);
            }
            int stripComponents = Integer.parseInt(
                    requiredProperty(lock, prefix + "stripComponents"));
            if (stripComponents < 0) {
                throw new IllegalArgumentException("negative stripComponents for " + id);
            }
            Path destination = safeRelativePath(
                    requiredProperty(lock, prefix + "destination"));
            List<Path> requiredPaths = Arrays.stream(
                            requiredProperty(lock, prefix + "requiredPaths").split(","))
                    .map(String::trim)
                    .filter(value -> !value.isEmpty())
                    .map(PrepareAndroidBinaryAssets::safeRelativePath)
                    .toList();
            artifacts.add(new Artifact(
                    id,
                    component,
                    url,
                    archiveName,
                    checksum,
                    stripComponents,
                    destination,
                    requiredPaths));
        }
        return artifacts;
    }

    private static String requiredProperty(Properties lock, String name) {
        String value = lock.getProperty(name);
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("missing lock property " + name);
        }
        return value.trim();
    }

    private static Path safeRelativePath(String rawPath) {
        Path path = Path.of(rawPath).normalize();
        if (path.isAbsolute()
                || path.getNameCount() == 0
                || path.startsWith("..")
                || rawPath.contains("\\")) {
            throw new IllegalArgumentException("unsafe relative path: " + rawPath);
        }
        return path;
    }

    private static Path resolveArchive(Artifact artifact, Path cache, boolean offline)
            throws Exception {
        String cacheName = artifact.id()
                + "-"
                + artifact.sha256().substring(0, 12)
                + "-"
                + artifact.archiveName();
        Path cachedArchive = cache.resolve(cacheName).normalize();
        if (!cachedArchive.startsWith(cache)) {
            throw new IllegalArgumentException("unsafe cache path for " + artifact.id());
        }

        if (Files.isRegularFile(cachedArchive)) {
            if (artifact.sha256().equals(sha256(cachedArchive))) {
                System.out.println("Using cached artifact: " + artifact.id());
                return cachedArchive;
            }
            Files.delete(cachedArchive);
        }

        if (offline) {
            throw new IOException(
                    "Gradle offline mode is enabled and the verified cache is missing: "
                            + artifact.id());
        }

        Files.createDirectories(cache);
        Path temporary = Files.createTempFile(cache, "." + artifact.id() + "-", ".part");
        try {
            System.out.println("Downloading " + artifact.url());
            HttpClient client = HttpClient.newBuilder()
                    .connectTimeout(Duration.ofSeconds(30))
                    .followRedirects(HttpClient.Redirect.ALWAYS)
                    .version(HttpClient.Version.HTTP_1_1)
                    .build();
            HttpRequest request = HttpRequest.newBuilder(artifact.url())
                    .timeout(Duration.ofMinutes(15))
                    .header("User-Agent", "actrium-android-binary-assets/1")
                    .GET()
                    .build();
            HttpResponse<InputStream> response =
                    client.send(request, HttpResponse.BodyHandlers.ofInputStream());
            try (InputStream body = response.body()) {
                if (response.statusCode() < 200 || response.statusCode() >= 300) {
                    throw new IOException(
                            "download failed for "
                                    + artifact.id()
                                    + ": HTTP "
                                    + response.statusCode());
                }
                copyWithProgress(body, temporary, artifact.id());
            }

            String actual = sha256(temporary);
            if (!artifact.sha256().equals(actual)) {
                throw new IOException(
                        "SHA-256 mismatch for "
                                + artifact.id()
                                + ": expected "
                                + artifact.sha256()
                                + ", got "
                                + actual);
            }
            moveReplacing(temporary, cachedArchive);
            return cachedArchive;
        } finally {
            Files.deleteIfExists(temporary);
        }
    }

    private static void copyWithProgress(InputStream input, Path destination, String artifactId)
            throws IOException {
        long totalBytes = 0;
        long nextProgressBytes = 10L * 1024 * 1024;
        byte[] buffer = new byte[1024 * 1024];
        try (OutputStream output = Files.newOutputStream(
                destination,
                StandardOpenOption.TRUNCATE_EXISTING,
                StandardOpenOption.WRITE)) {
            int count;
            while ((count = input.read(buffer)) >= 0) {
                if (count == 0) {
                    continue;
                }
                output.write(buffer, 0, count);
                totalBytes += count;
                if (totalBytes >= nextProgressBytes) {
                    System.out.println(
                            "Downloaded "
                                    + artifactId
                                    + ": "
                                    + (totalBytes / (1024 * 1024))
                                    + " MiB");
                    nextProgressBytes += 10L * 1024 * 1024;
                }
            }
        }
    }

    private static void extractArchive(Artifact artifact, Path archive, Path staging)
            throws IOException {
        Path destination = staging.resolve(artifact.destination()).normalize();
        if (!destination.startsWith(staging)) {
            throw new IOException("unsafe extraction destination for " + artifact.id());
        }
        Files.createDirectories(destination);

        int extractedFiles = 0;
        try (ZipInputStream zip = new ZipInputStream(Files.newInputStream(archive))) {
            ZipEntry entry;
            while ((entry = zip.getNextEntry()) != null) {
                String normalizedName = entry.getName().replace('\\', '/');
                if (normalizedName.startsWith("/")) {
                    throw new IOException("absolute ZIP entry in " + artifact.id());
                }
                List<String> segments = Arrays.stream(normalizedName.split("/"))
                        .filter(segment -> !segment.isEmpty())
                        .toList();
                if (segments.stream().anyMatch(segment -> segment.equals(".."))) {
                    throw new IOException("parent traversal ZIP entry in " + artifact.id());
                }
                if (segments.size() <= artifact.stripComponents()) {
                    zip.closeEntry();
                    continue;
                }

                Path relative = Path.of(
                        segments.get(artifact.stripComponents()),
                        segments.subList(
                                        artifact.stripComponents() + 1,
                                        segments.size())
                                .toArray(String[]::new));
                Path target = destination.resolve(relative).normalize();
                if (!target.startsWith(destination)) {
                    throw new IOException("ZIP entry escapes destination in " + artifact.id());
                }

                if (entry.isDirectory()) {
                    Files.createDirectories(target);
                } else {
                    Files.createDirectories(target.getParent());
                    Files.copy(zip, target, StandardCopyOption.REPLACE_EXISTING);
                    extractedFiles++;
                }
                zip.closeEntry();
            }
        }
        if (extractedFiles == 0) {
            throw new IOException("archive extracted no files: " + artifact.id());
        }
    }

    private static void verifyRequiredPaths(Path staging, List<Artifact> artifacts)
            throws IOException {
        for (Artifact artifact : artifacts) {
            for (Path required : artifact.requiredPaths()) {
                Path candidate = staging.resolve(required).normalize();
                if (!candidate.startsWith(staging) || !Files.isRegularFile(candidate)) {
                    throw new IOException(
                            "artifact "
                                    + artifact.id()
                                    + " is missing required file "
                                    + required);
                }
            }
        }
    }

    private static boolean isPrepared(
            Path output, String signature, List<Artifact> artifacts) throws IOException {
        Path marker = output.resolve(MARKER_FILE);
        if (!Files.isRegularFile(marker)) {
            return false;
        }
        if (!signature.equals(Files.readString(marker, StandardCharsets.UTF_8).trim())) {
            return false;
        }
        for (Artifact artifact : artifacts) {
            for (Path required : artifact.requiredPaths()) {
                if (!Files.isRegularFile(output.resolve(required))) {
                    return false;
                }
            }
        }
        return true;
    }

    private static void replaceGeneratedDirectory(Path output, Path staging)
            throws IOException {
        if (Files.exists(output)) {
            Path marker = output.resolve(MARKER_FILE);
            boolean empty;
            try (var children = Files.list(output)) {
                empty = children.findAny().isEmpty();
            }
            if (!empty && !Files.isRegularFile(marker)) {
                throw new IOException(
                        "refusing to replace non-generated output directory: " + output);
            }
            deleteRecursively(output);
        }
        moveReplacing(staging, output);
    }

    private static void moveReplacing(Path source, Path destination) throws IOException {
        try {
            Files.move(
                    source,
                    destination,
                    StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING);
        } catch (AtomicMoveNotSupportedException ignored) {
            Files.move(source, destination, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static void deleteRecursively(Path path) throws IOException {
        if (!Files.exists(path)) {
            return;
        }
        try (var paths = Files.walk(path)) {
            for (Path candidate : paths.sorted(Comparator.reverseOrder()).toList()) {
                Files.deleteIfExists(candidate);
            }
        }
    }

    private static String sha256(Path file) throws IOException {
        MessageDigest digest;
        try {
            digest = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException error) {
            throw new IllegalStateException("SHA-256 is unavailable", error);
        }
        try (InputStream input = Files.newInputStream(file)) {
            byte[] buffer = new byte[1024 * 1024];
            int count;
            while ((count = input.read(buffer)) >= 0) {
                if (count > 0) {
                    digest.update(buffer, 0, count);
                }
            }
        }
        return HexFormat.of().formatHex(digest.digest());
    }
}
