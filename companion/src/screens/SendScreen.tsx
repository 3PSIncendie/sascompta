import * as DocumentPicker from "expo-document-picker";
import * as ImagePicker from "expo-image-picker";
import { useState } from "react";
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { uploadDocument } from "../api";
import { DOC_TYPE_OPTIONS, type DocTypeHint, type Session } from "../types";

type Props = {
  session: Session;
  onForget: () => void;
};

async function readUriAsBytes(uri: string): Promise<Uint8Array> {
  const res = await fetch(uri);
  const buf = await res.arrayBuffer();
  return new Uint8Array(buf);
}

export function SendScreen({ session, onForget }: Props) {
  const [docType, setDocType] = useState<DocTypeHint>("expense_receipt");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ok, setOk] = useState<string | null>(null);

  async function send(
    bytes: Uint8Array,
    fileName: string,
    mimeType: string,
  ) {
    setBusy(true);
    setError(null);
    setOk(null);
    try {
      const result = await uploadDocument(session, bytes, {
        fileName,
        mimeType,
        docType,
      });
      setOk(
        result.message ||
          `Document #${result.id} envoyé — à revoir sur le PC avant écriture.`,
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function takePhoto() {
    const perm = await ImagePicker.requestCameraPermissionsAsync();
    if (!perm.granted) {
      setError("Autorisation caméra refusée.");
      return;
    }
    const shot = await ImagePicker.launchCameraAsync({
      quality: 0.85,
      allowsEditing: false,
    });
    if (shot.canceled || !shot.assets[0]) return;
    const asset = shot.assets[0];
    const bytes = await readUriAsBytes(asset.uri);
    const name = asset.fileName || `photo_${Date.now()}.jpg`;
    await send(bytes, name, asset.mimeType || "image/jpeg");
  }

  async function pickImage() {
    const perm = await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (!perm.granted) {
      setError("Autorisation photos refusée.");
      return;
    }
    const pick = await ImagePicker.launchImageLibraryAsync({
      quality: 0.9,
      allowsEditing: false,
    });
    if (pick.canceled || !pick.assets[0]) return;
    const asset = pick.assets[0];
    const bytes = await readUriAsBytes(asset.uri);
    const name = asset.fileName || `image_${Date.now()}.jpg`;
    await send(bytes, name, asset.mimeType || "image/jpeg");
  }

  async function pickFile() {
    const pick = await DocumentPicker.getDocumentAsync({
      type: ["application/pdf", "image/*"],
      copyToCacheDirectory: true,
      multiple: false,
    });
    if (pick.canceled || !pick.assets[0]) return;
    const asset = pick.assets[0];
    const bytes = await readUriAsBytes(asset.uri);
    await send(
      bytes,
      asset.name || `document_${Date.now()}.pdf`,
      asset.mimeType || "application/pdf",
    );
  }

  return (
    <ScrollView contentContainerStyle={styles.wrap}>
      <View style={styles.card}>
        <Text style={styles.cardLabel}>Connecté à</Text>
        <Text style={styles.org}>{session.orgName}</Text>
        <Text style={styles.endpoint}>
          {session.host}:{session.port}
        </Text>
      </View>

      <Text style={styles.section}>Type de document (indice)</Text>
      <View style={styles.chips}>
        {DOC_TYPE_OPTIONS.map((opt) => {
          const active = opt.value === docType;
          return (
            <Pressable
              key={opt.value}
              style={[styles.chip, active && styles.chipActive]}
              onPress={() => setDocType(opt.value)}
            >
              <Text style={[styles.chipText, active && styles.chipTextActive]}>
                {opt.label}
              </Text>
            </Pressable>
          );
        })}
      </View>

      {error ? <Text style={styles.error}>{error}</Text> : null}
      {ok ? <Text style={styles.ok}>{ok}</Text> : null}

      <Pressable
        style={[styles.primaryBtn, busy && styles.disabled]}
        disabled={busy}
        onPress={() => void takePhoto()}
      >
        <Text style={styles.primaryBtnText}>
          {busy ? "Envoi…" : "Prendre une photo"}
        </Text>
      </Pressable>
      <Pressable
        style={[styles.secondaryBtn, busy && styles.disabled]}
        disabled={busy}
        onPress={() => void pickImage()}
      >
        <Text style={styles.secondaryBtnText}>Choisir une image</Text>
      </Pressable>
      <Pressable
        style={[styles.secondaryBtn, busy && styles.disabled]}
        disabled={busy}
        onPress={() => void pickFile()}
      >
        <Text style={styles.secondaryBtnText}>Choisir un PDF / fichier</Text>
      </Pressable>

      <Pressable style={styles.forget} onPress={onForget}>
        <Text style={styles.forgetText}>Oublier ce PC</Text>
      </Pressable>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  wrap: { padding: 20, gap: 10, paddingBottom: 40 },
  card: {
    backgroundColor: "#fff",
    borderRadius: 16,
    padding: 16,
    borderWidth: 1,
    borderColor: "#c5d9cc",
    marginBottom: 6,
  },
  cardLabel: { color: "#4d6b5c", fontSize: 12, fontWeight: "600" },
  org: { marginTop: 4, fontSize: 20, fontWeight: "800", color: "#1a3d2e" },
  endpoint: {
    marginTop: 4,
    fontFamily: "monospace",
    color: "#355647",
    fontSize: 13,
  },
  section: {
    marginTop: 8,
    fontWeight: "700",
    color: "#1a3d2e",
    fontSize: 15,
  },
  chips: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  chip: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "#c5d9cc",
    backgroundColor: "#fff",
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  chipActive: {
    backgroundColor: "#3a7d5c",
    borderColor: "#3a7d5c",
  },
  chipText: { color: "#1a3d2e", fontSize: 13, fontWeight: "600" },
  chipTextActive: { color: "#fff" },
  primaryBtn: {
    marginTop: 10,
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
  },
  secondaryBtnText: { color: "#8a4b1a", fontWeight: "700", fontSize: 15 },
  disabled: { opacity: 0.55 },
  error: { color: "#a33a2c" },
  ok: { color: "#1f6b45", fontWeight: "600" },
  forget: { marginTop: 18, alignItems: "center", padding: 10 },
  forgetText: { color: "#6b7c72", textDecorationLine: "underline" },
});
