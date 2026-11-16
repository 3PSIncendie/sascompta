import { StatusBar } from "expo-status-bar";
import { useCallback, useEffect, useState } from "react";
import {
  ActivityIndicator,
  SafeAreaView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { PairScreen } from "./src/screens/PairScreen";
import { SendScreen } from "./src/screens/SendScreen";
import { clearSession, loadSession, saveSession } from "./src/storage";
import type { Session } from "./src/types";

export default function App() {
  const [booting, setBooting] = useState(true);
  const [session, setSession] = useState<Session | null>(null);

  useEffect(() => {
    void loadSession()
      .then(setSession)
      .finally(() => setBooting(false));
  }, []);

  const onPaired = useCallback(async (s: Session) => {
    await saveSession(s);
    setSession(s);
  }, []);

  const onForget = useCallback(async () => {
    await clearSession();
    setSession(null);
  }, []);

  if (booting) {
    return (
      <View style={styles.boot}>
        <ActivityIndicator color="#3a7d5c" />
        <Text style={styles.bootText}>SasCompta compagnon…</Text>
      </View>
    );
  }

  return (
    <SafeAreaView style={styles.root}>
      <StatusBar style="dark" />
      <View style={styles.brand}>
        <Text style={styles.brandName}>SasCompta</Text>
        <Text style={styles.brandSub}>Compagnon · Wi‑Fi local</Text>
      </View>
      {session ? (
        <SendScreen session={session} onForget={() => void onForget()} />
      ) : (
        <PairScreen onPaired={(s) => void onPaired(s)} />
      )}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  root: {
    flex: 1,
    backgroundColor: "#eef7f1",
  },
  boot: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 12,
    backgroundColor: "#eef7f1",
  },
  bootText: { color: "#1a3d2e", fontSize: 15 },
  brand: {
    paddingHorizontal: 20,
    paddingTop: 12,
    paddingBottom: 4,
  },
  brandName: {
    fontSize: 28,
    fontWeight: "800",
    color: "#1a3d2e",
    letterSpacing: -0.5,
  },
  brandSub: {
    marginTop: 2,
    fontSize: 14,
    color: "#4d6b5c",
  },
});
