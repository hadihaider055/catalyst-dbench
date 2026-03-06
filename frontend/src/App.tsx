import { useEffect } from "react";
import Layout from "@/components/Layout";
import { useAppStore } from "@/stores/useAppStore";
import { listConnections } from "@/lib/commands";

export default function App() {
  const setActiveConnections = useAppStore((s) => s.setActiveConnections);

  useEffect(() => {
    listConnections()
      .then(setActiveConnections)
      .catch(() => {/* app may not be connected to backend yet */});
  }, [setActiveConnections]);

  return <Layout />;
}
