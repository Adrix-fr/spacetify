#!/bin/sh
# Installs spacetify on macOS and Linux, in one line from a terminal:
#
#   curl -fsSL https://raw.githubusercontent.com/spacetify-mod/spacetify/main/install.sh | sh
#
# It downloads the package of the latest release from GitHub (.dmg on macOS,
# .deb or .rpm on Linux), checks it against the SHA-256 published with the
# release, then installs it. Nothing is installed if the check fails.
# On Linux, the system asks for your password once, like for any package.
#
# To install a given version instead of the latest one:
#   curl -fsSL https://raw.githubusercontent.com/spacetify-mod/spacetify/main/install.sh | SPACETIFY_VERSION=v0.1.0 sh

set -eu

# Everything is in one function, so the whole script is read before any of it
# runs: with `curl | sh`, nothing the installers do can eat the rest of it.
main() {

REPO="spacetify-mod/spacetify"

case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in
  fr*) FR=1 ;;
  *) FR=0 ;;
esac

say() {
  if [ "$FR" = 1 ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi
}

fail() {
  printf '\n' >&2
  if [ "$FR" = 1 ]; then printf '%s\n' "$2" >&2; else printf '%s\n' "$1" >&2; fi
  printf '%s\n' "https://github.com/$REPO/releases" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1
}

need curl || fail "curl is needed." "curl est nécessaire."

# Which package this computer needs.
OS="$(uname -s)"
case "$OS" in
  Darwin)
    KIND=dmg
    PATTERN='\.dmg$'
    ;;
  Linux)
    case "$(uname -m)" in
      x86_64 | amd64) ;;
      *) fail "On Linux, spacetify is only built for 64-bit Intel and AMD processors." \
              "Sous Linux, spacetify n'existe que pour les processeurs Intel et AMD 64 bits." ;;
    esac
    if need apt-get; then
      KIND=deb
      PATTERN='_amd64\.deb$'
    elif need dnf; then
      KIND=dnf
      PATTERN='\.x86_64\.rpm$'
    elif need zypper; then
      KIND=zypper
      PATTERN='\.x86_64\.rpm$'
    else
      fail "spacetify is packaged for apt (Ubuntu, Debian, Mint), dnf (Fedora) and zypper (openSUSE). None of them was found." \
           "spacetify existe pour apt (Ubuntu, Debian, Mint), dnf (Fedora) et zypper (openSUSE). Aucun des trois n'a été trouvé."
    fi
    ;;
  *)
    fail "spacetify works on Windows, macOS and Linux. On Windows, use install.ps1." \
         "spacetify fonctionne sous Windows, macOS et Linux. Sous Windows, utilise install.ps1."
    ;;
esac

if need sha256sum; then
  SHA="sha256sum"
elif need shasum; then
  SHA="shasum -a 256"
else
  fail "sha256sum or shasum is needed to check the download." \
       "sha256sum ou shasum est nécessaire pour vérifier le téléchargement."
fi

if [ -n "${SPACETIFY_VERSION:-}" ]; then
  API="https://api.github.com/repos/$REPO/releases/tags/$SPACETIFY_VERSION"
else
  API="https://api.github.com/repos/$REPO/releases/latest"
fi

TMP="$(mktemp -d)"
cleanup() {
  if [ -n "${MOUNT:-}" ]; then hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true; fi
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

say "Looking for the latest version of spacetify..." "Recherche de la dernière version de spacetify..."
CODE="$(curl -sSL -o "$TMP/release.json" -w '%{http_code}' -H 'User-Agent: spacetify-install' "$API" || true)"
case "$CODE" in
  200) ;;
  404)
    if [ -n "${SPACETIFY_VERSION:-}" ]; then
      fail "There is no version $SPACETIFY_VERSION of spacetify." "Il n'existe pas de version $SPACETIFY_VERSION de spacetify."
    else
      fail "No version of spacetify has been released yet." "Aucune version de spacetify n'a encore été publiée."
    fi
    ;;
  *) fail "Could not reach GitHub. Check your connection and try again." \
          "Impossible de joindre GitHub. Vérifie ta connexion et réessaie." ;;
esac

# The download addresses of the release's files, one per line.
URLS="$(tr ',' '\n' < "$TMP/release.json" | sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
TAG="$(tr ',' '\n' < "$TMP/release.json" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"

URL="$(printf '%s\n' "$URLS" | grep -E "$PATTERN" | head -n 1 || true)"
[ -n "$URL" ] || fail "Version $TAG has no package for this system." "La version $TAG n'a pas de paquet pour ce système."
NAME="${URL##*/}"

# The SHA-256 of every file is published with the release.
EXPECTED=""
for SUMS in $(printf '%s\n' "$URLS" | grep -E '/SHA256SUMS[^/]*\.txt$' || true); do
  curl -fsSL -H 'User-Agent: spacetify-install' "$SUMS" -o "$TMP/sums.txt" || continue
  EXPECTED="$(tr -d '\r' < "$TMP/sums.txt" | awk -v n="$NAME" '{ f = $2; sub(/^\*/, "", f); if (f == n) { print tolower($1); exit } }')"
  [ -n "$EXPECTED" ] && break
done
[ -n "$EXPECTED" ] || fail "The SHA-256 of the package is not published, so it cannot be checked. Nothing was installed." \
                           "L'empreinte SHA-256 du paquet n'est pas publiée, il ne peut donc pas être vérifié. Rien n'a été installé."

say "Downloading spacetify $TAG..." "Téléchargement de spacetify $TAG..."
curl -fSL --progress-bar -H 'User-Agent: spacetify-install' "$URL" -o "$TMP/$NAME" \
  || fail "The download failed. Check your connection and try again." "Le téléchargement a échoué. Vérifie ta connexion et réessaie."

ACTUAL="$($SHA "$TMP/$NAME" | awk '{ print tolower($1) }')"
[ "$ACTUAL" = "$EXPECTED" ] || fail "The downloaded file does not match its SHA-256. Nothing was installed." \
                                    "Le fichier téléchargé ne correspond pas à son empreinte SHA-256. Rien n'a été installé."
say "File checked (SHA-256)." "Fichier vérifié (SHA-256)."

as_root() {
  if [ "$(id -u)" = 0 ]; then "$@"; else sudo "$@"; fi
}

case "$KIND" in
  deb)
    say "Installing the package (your password may be asked)..." "Installation du paquet (ton mot de passe peut être demandé)..."
    as_root apt-get install -y "$TMP/$NAME" </dev/null
    ;;
  dnf)
    say "Installing the package (your password may be asked)..." "Installation du paquet (ton mot de passe peut être demandé)..."
    as_root dnf install -y "$TMP/$NAME" </dev/null
    ;;
  zypper)
    say "Installing the package (your password may be asked)..." "Installation du paquet (ton mot de passe peut être demandé)..."
    as_root zypper --non-interactive install --allow-unsigned-rpm "$TMP/$NAME" </dev/null
    ;;
  dmg)
    MOUNT="$TMP/mount"
    mkdir -p "$MOUNT"
    hdiutil attach "$TMP/$NAME" -mountpoint "$MOUNT" -nobrowse -readonly -quiet
    APP="$(find "$MOUNT" -maxdepth 1 -name '*.app' | head -n 1)"
    [ -n "$APP" ] || fail "The .dmg holds no app." "Le .dmg ne contient pas d'appli."
    TARGET="/Applications/${APP##*/}"
    say "Copying spacetify into Applications..." "Copie de spacetify dans Applications..."
    osascript -e 'quit app "spacetify"' >/dev/null 2>&1 || true
    if [ -w /Applications ]; then
      rm -rf "$TARGET"
      ditto "$APP" "$TARGET"
    else
      sudo rm -rf "$TARGET"
      sudo ditto "$APP" "$TARGET"
    fi
    hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true
    MOUNT=""
    open "$TARGET" || true
    ;;
esac

printf '\n'
case "$KIND" in
  dmg)
    say "Done. spacetify is in Applications. If macOS says it cannot verify it, open System Settings > Privacy & Security and click Open Anyway." \
        "C'est fini. spacetify est dans Applications. Si macOS dit qu'il ne peut pas le vérifier, ouvre Réglages Système > Confidentialité et sécurité et clique sur Ouvrir quand même."
    ;;
  *)
    say "Done. Open spacetify from your applications menu, or type spacetify in a terminal." \
        "C'est fini. Ouvre spacetify depuis le menu des applications, ou tape spacetify dans un terminal."
    ;;
esac
}

main "$@"
