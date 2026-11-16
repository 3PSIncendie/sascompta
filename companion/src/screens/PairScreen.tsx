import { CameraView, useCameraPermissions } from "expo-camera";
import { useState } from "react";
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { fetchInfo, pair, parsePairingPayload } from "../api";
import type { Session } from "../types";

type Props = {
  onPaired: (session: Session) => void;
};

export function PairScreen({ onPaired }: Props) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("17832");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [scan, setScan] = useState(false);
  const [permission, requestPermission] = useCameraPermissions();

  async function doPair(h: string, p: number, c: string) {
    setBusy(true);
    setError(null);
    try {
      await fetchInfo(h, p);
      const session = await pair(h, p, c);
      onPaired(session);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onSubmit() {
    const h = host.trim();
    const p = Number(port);
    if (!h || !Number.isFinite(p) || p <= 0) {
      setError("Indiquez l’IP du PC et le port (ex. 17832).");
      return;
    }
    if (!/^\d{6}$/.test(code.trim())) {
      setError("Le code d’appairage comporte 6 chiffres.");
      return;
    }
    await doPair(h, p, code.trim());
  }

  async function openScanner() {
    if (!permission?.granted) {
      const res = await requestPermission();
      if (!res.granted) {
        setError("Autorisation caméra refusée — saisissez le code à la main.");
        return;
      }
    }
    setScan(true);
    setError(null);
  }

  return (
    <View style={styles.wrap}>
      <Text style={styles.lead}>
        Sur le PC : Ma SAS ou Import docs → « Activer le pont Wi‑Fi ». Même
        réseau Wi‑Fi, puis scannez le QR ou tapez le code.
      </Text>

      {scan ? (
        <View style={styles.scannerBox}>
          <CameraView
            style={StyleSheet.absoluteFillObject}
            barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
            onBarcodeScanned={({ data }) => {
              setScan(false);
              try {
                const payload = parsePairingPayload(data);
                setHost(payload.host);
                setPort(String(payload.port));
                setCode(payload.code);
                void doPair(payload.host, payload.port, payload.code);
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              }
            }}
          />
          <Pressable style={styles.scanCancel} onPress={() => setScan(false)}>
            <Text style={styles.scanCancelText}>Fermer le scan</Text>
          </Pressable>
        </View>
      ) : (
        <>
          <Pressable style={styles.secondaryBtn} onPress={() => void openScanner()}>
            <Text style={styles.secondaryBtnText}>Scanner le QR</Text>
          </Pressable>

          <Text style={styles.label}>Adresse IP du PC</Text>
          <TextInput
            style={styles.input}
            value={host}
            onChangeText={setHost}
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="numbers-and-punctuation"
            placeholder="192.168.1.42"
            placeholderTextColor="#8aa396"
          />

          <Text style={styles.label}>Port</Text>
          <TextInput
            style={styles.input}
            value={port}
            onChangeText={setPort}
            keyboardType="number-pad"
            placeholder="17832"
            placeholderTextColor="#8aa396"
          />

          <Text style={styles.label}>Code d’appairage (6 chiffres)</Text>
          <TextInput
            style={[styles.input, styles.codeInput]}
            value={code}
            onChangeText={setCode}
            keyboardType="number-pad"
            maxLength={6}
            placeholder="482913"
            placeholderTextColor="#8aa396"
          />

          {error ? <Text style={styles.error}>{error}</Text> : null}

          <Pressable
            style={[styles.primaryBtn, busy && styles.disabled]}
            disabled={busy}
            onPress={() => void onSubmit()}
          >
            <Text style={styles.primaryBtnText}>
              {busy ? "Appairage…" : "Appairer"}
            </Text>
          </Pressable>
        </>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { flex: 1, padding: 20, gap: 8 },
  lead: { color: "#355647", fontSize: 15, lineHeight: 22, marginBottom: 8 },
  label: { marginTop: 6, color: "#1a3d2e", fontWeight: "600", fontSize: 13 },
  input: {
    backgroundColor: "#fff",
    borderWidth: 1,
    borderColor: "#c5d9cc",
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 12,
    fontSize: 16,
    color: "#1a3d2e",
  },
  codeInput: {
    letterSpacing: 6,
    fontSize: 22,
    fontWeight: "700",
    textAlign: "center",
  },
  primaryBtn: {
    marginTop: 14,
    backgroundColor: "#3a7d5c",
    borderRadius: 14,
    paddingVertical: 14,
    alignItems: "center",
  },
  primaryBtnText: { color: "#fff", fontWeight: "700", fontSize: 16 },
  secondaryBtn: {
    backgroundColor: "#fff4e8",
    borderRadius: 14,
    paddingVertical: 12,
    alignItems: "center",
    borderWidth: 1,
    borderColor: "#f2b27a",
    marginBottom: 8,
  },
  secondaryBtnText: { color: "#8a4b1a", fontWeight: "700", fontSize: 15 },
  disabled: { opacity: 0.6 },
  error: { color: "#a33a2c", marginTop: 6 },
  scannerBox: {
    height: 320,
    borderRadius: 16,
    overflow: "hidden",
    backgroundColor: "#111",
  },
  scanCancel: {
    position: "absolute",
    bottom: 12,
    alignSelf: "center",
    backgroundColor: "rgba(0,0,0,0.65)",
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderRadius: 999,
  },
  scanCancelText: { color: "#fff", fontWeight: "600" },
});
