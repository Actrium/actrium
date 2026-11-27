type ErrorCopy = { title: string; message: string };

const openTitle = 'Could not open the application';
const incompatible: ErrorCopy = {
  title: 'This application is incompatible',
  message: 'This version of Actrium does not support a feature the application needs.',
};
const verification: ErrorCopy = {
  title: 'Actrium blocked this application',
  message: 'The application package could not be verified.',
};
const fallback: ErrorCopy = {
  title: openTitle,
  message: 'An unexpected error occurred while opening this application.',
};
const copy: Readonly<Record<string, ErrorCopy>> = {
  ACTR_LINK_INVALID: { title: openTitle, message: 'Check the Actr link and try opening it again.' },
  ACTR_REALM_RETIRED: { title: openTitle, message: 'This Realm is no longer available.' },
  ACTR_INSTANCE_NOT_FOUND: { title: openTitle, message: 'This local instance could not be found.' },
  ACTR_REALM_UNVERIFIED: { title: openTitle, message: 'The Realm information could not be verified.' },
  PACKAGE_NOT_FOUND: { title: openTitle, message: 'The application package could not be found.' },
  PACKAGE_INVALID: { title: openTitle, message: 'The application package could not be read.' },
  PACKAGE_DOWNLOAD_FAILED: { title: openTitle, message: 'Check your connection and try opening the application again.' },
  PACKAGE_TOO_LARGE: { title: openTitle, message: 'The application package exceeds the download size limit.' },
  PACKAGE_SIGNATURE_INVALID: verification,
  PACKAGE_DIGEST_MISMATCH: verification,
  FRONT_VERIFICATION_FAILED: verification,
  PUBLISHER_IDENTITY_CHANGED: {
    title: 'Actrium blocked this application',
    message: 'The publisher identity changed. Your saved data has not been shared.',
  },
  PUBLISHER_UNVERIFIED: {
    title: 'Actrium blocked this application',
    message: 'The application publisher could not be verified.',
  },
  ACTR_STORAGE_FAILED: { title: openTitle, message: 'The application’s saved data could not be accessed.' },
  component_not_allowed: incompatible,
  unknown_capability: incompatible,
  ACTR_HOST_FEATURE_MISSING: incompatible,
  capability_mismatch: { title: openTitle, message: 'The application’s declared capabilities could not be verified.' },
  GUEST_START_FAILED: { title: openTitle, message: 'The application could not start.' },
  GUEST_ROOT_CRASHED: {
    title: 'The application stopped unexpectedly',
    message: 'Reload the tab to open the application again.',
  },
};

export function describeActrError(value: unknown): ErrorCopy & { code: string } {
  const code =
    typeof value === 'string' && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(value)
      ? value
      : 'ACTR_OPEN_FAILED';
  const text = Object.prototype.hasOwnProperty.call(copy, code) ? copy[code] : fallback;
  return { ...text, code };
}
