#include "ActrArchive.h"

#include <archive.h>
#include <archive_entry.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

// Resolve each component relative to an already opened directory. Never follow
// links or permit an archive to overwrite existing files, even with aliases.
static int open_entry(int root, const char *name, int directory,
                      uint64_t *nodes, uint64_t maximum_nodes) {
    if (!name || !*name || name[0] == '/' || strlen(name) >= PATH_MAX) return -1;
    char path[PATH_MAX];
    strcpy(path, name);
    int parent = dup(root);
    if (parent < 0) return -1;
    char *state = NULL;
    char *part = strtok_r(path, "/", &state);
    while (part) {
        char *next = strtok_r(NULL, "/", &state);
        if (strcmp(part, "..") == 0 || strchr(part, '\\')) break;
        if (strcmp(part, ".") == 0) {
            part = next;
            if (!part && directory) return parent;
            continue;
        }
        if (next || directory) {
            struct stat info;
            if (fstatat(parent, part, &info, AT_SYMLINK_NOFOLLOW) != 0) {
                if (errno != ENOENT || *nodes == maximum_nodes ||
                    mkdirat(parent, part, 0700) != 0) break;
                ++*nodes;
            }
            int child = openat(parent, part, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
            if (child < 0) break;
            close(parent);
            parent = child;
            if (!next) return parent;
        } else {
            if (*nodes == maximum_nodes) break;
            int file = openat(parent, part, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
            if (file >= 0) ++*nodes;
            close(parent);
            return file;
        }
        part = next;
    }
    close(parent);
    return -1;
}

int actr_extract_zip(const char *source, const char *destination,
                     uint64_t maximum_bytes, uint64_t maximum_entries,
                     char *error, size_t error_capacity) {
    struct archive *reader = archive_read_new();
    int root = -1, output = -1, result = -1;
    const char *failure = "Unable to read Actor package archive";
    if (!reader) goto cleanup;
    root = open(destination, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (root < 0) goto cleanup;
    if (archive_read_support_filter_none(reader) != ARCHIVE_OK ||
        archive_read_support_format_zip(reader) != ARCHIVE_OK ||
        archive_read_open_filename(reader, source, 65536) != ARCHIVE_OK) goto cleanup;

    uint64_t bytes = 0, entries = 0, nodes = 0;
    struct archive_entry *entry = NULL;
    int status;
    while ((status = archive_read_next_header(reader, &entry)) == ARCHIVE_OK) {
        if (entries == maximum_entries) {
            failure = "Actor package contains too many archive entries";
            goto cleanup;
        }
        ++entries;
        int directory = archive_entry_filetype(entry) == AE_IFDIR;
        if ((!directory && archive_entry_filetype(entry) != AE_IFREG) ||
            archive_entry_symlink(entry) || archive_entry_hardlink(entry)) {
            failure = "Actor package contains a link or unsupported file type";
            goto cleanup;
        }
        if (archive_entry_size(entry) < 0 ||
            (uint64_t)archive_entry_size(entry) > maximum_bytes - bytes) {
            failure = "Actor package exceeds the extracted byte limit";
            goto cleanup;
        }
        output = open_entry(root, archive_entry_pathname(entry), directory, &nodes, maximum_entries);
        if (output < 0) {
            failure = "Actor package contains an unsafe, duplicate, or unwritable path";
            goto cleanup;
        }
        char buffer[65536];
        la_ssize_t count;
        // Count actual decoded bytes, independently of the archive's size fields.
        // Also read directory bodies so malformed entries cannot bypass the cap.
        while ((count = archive_read_data(reader, buffer, sizeof(buffer))) > 0) {
            if ((uint64_t)count > maximum_bytes - bytes) {
                failure = "Actor package exceeds the extracted byte limit";
                goto cleanup;
            }
            bytes += (uint64_t)count;
            if (directory) {
                failure = "Actor package contains a directory with file data";
                goto cleanup;
            }
            for (la_ssize_t offset = 0; offset < count;) {
                ssize_t written = write(output, buffer + offset, (size_t)(count - offset));
                if (written < 0 && errno == EINTR) continue;
                if (written <= 0) {
                    failure = "Unable to write extracted Actor package";
                    goto cleanup;
                }
                offset += written;
            }
        }
        if (count < 0) goto cleanup;
        int closed = close(output);
        output = -1;
        if (closed != 0) goto cleanup;
    }
    if (status != ARCHIVE_EOF) goto cleanup;
    result = 0;

cleanup:
    if (result != 0 && error_capacity > 0) snprintf(error, error_capacity, "%s", failure);
    if (output >= 0) close(output);
    if (root >= 0) close(root);
    if (reader) archive_read_free(reader);
    return result;
}
