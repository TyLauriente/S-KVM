import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppConfig } from "../types";

export function useConfig() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<AppConfig>("get_config");
      setConfig(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const saveConfig = useCallback(
    async (newConfig: AppConfig): Promise<boolean> => {
      try {
        setError(null);
        await invoke("save_config", { config: newConfig });
        setConfig(newConfig);
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [],
  );

  const updateConfig = useCallback(
    (updater: (prev: AppConfig) => AppConfig) => {
      setConfig((prev) => (prev ? updater(prev) : prev));
    },
    [],
  );

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  return { config, loading, error, saveConfig, updateConfig, reload: loadConfig };
}
