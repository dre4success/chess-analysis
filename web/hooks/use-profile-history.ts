import { useCallback, useEffect, useRef, useState } from 'react';
import {
  PROFILE_HISTORY_KEY,
  readProfileHistory,
  rememberProfile,
} from '../lib/profile-history';

function browserStorage() {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function useProfileHistory() {
  const [profiles, setProfiles] = useState(() => readProfileHistory(browserStorage()));
  const current = useRef(profiles);
  useEffect(() => {
    const changed = (event: StorageEvent) => {
      if (event.storageArea !== browserStorage()) return;
      if (event.key !== null && event.key !== PROFILE_HISTORY_KEY) return;
      const next = readProfileHistory(browserStorage());
      current.current = next;
      setProfiles(next);
    };
    window.addEventListener('storage', changed);
    return () => window.removeEventListener('storage', changed);
  }, []);
  const remember = useCallback((username: string) => {
    const next = rememberProfile(browserStorage(), username, current.current);
    current.current = next;
    setProfiles(next);
  }, []);
  return { profiles, remember };
}
