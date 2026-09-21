#ifndef ACTR_ARCHIVE_H
#define ACTR_ARCHIVE_H

#include <stddef.h>
#include <stdint.h>

// destination must be an empty private directory owned by the caller.
// On failure the caller removes the entire destination, including partial files.
int actr_extract_zip(const char *source, const char *destination,
                     uint64_t maximum_bytes, uint64_t maximum_entries,
                     char *error, size_t error_capacity);

#endif
