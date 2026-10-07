import { useEffect } from "react";
import Layout from "@/components/Layout";
import { useAppStore } from "@/stores/useAppStore";
import { listConnections } from "@/lib/commands";

export default function App() {
  const setActiveConnections = useAppStore((s) => s.setActiveConnections);

  // Once saved state has loaded, move any passwords still embedded in URIs to the keychain.
  useEffect(() => {
    const secure = () => void useAppStore.getState().secureSavedConnections();
    if (useAppStore.persist.hasHydrated()) secure();
    else return useAppStore.persist.onFinishHydration(secure);
  }, []);

  useEffect(() => {
    listConnections()
      .then(setActiveConnections)
      .catch(() => {
        /* app may not be connected to backend yet */
      });
  }, [setActiveConnections]);

  return <Layout />;
}
