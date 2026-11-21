#!/usr/bin/env bash
set -euo pipefail

test -n "${IOS_CERTIFICATE_BASE64:?Apple Distribution certificate required}"
test -n "${IOS_PROFILE_BASE64:?App Store Connect provisioning profile required}"
: "${IOS_CERTIFICATE_PASSWORD?Certificate password secret required}"

umask 077
signing_dir="$(mktemp -d "$RUNNER_TEMP/slint-signing.XXXXXX")"
keychain="$signing_dir/ios.keychain-db"
installed_profile=""
cleanup() {
    security delete-keychain "$keychain" >/dev/null 2>&1 || true
    if [[ -n "$installed_profile" ]]; then rm -f "$installed_profile"; fi
    rm -rf "$signing_dir"
}
trap cleanup EXIT

printf '%s' "$IOS_CERTIFICATE_BASE64" | base64 -D > "$signing_dir/certificate.p12"
printf '%s' "$IOS_PROFILE_BASE64" | base64 -D > "$signing_dir/app.mobileprovision"
security cms -D -i "$signing_dir/app.mobileprovision" > "$signing_dir/profile.plist"
profile_uuid="$(/usr/libexec/PlistBuddy -c 'Print :UUID' "$signing_dir/profile.plist")"
team="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$signing_dir/profile.plist")"
bundle_id=com.caleb.offeringrecorder.slint
app_id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$signing_dir/profile.plist")"
[[ "$profile_uuid" =~ ^[[:xdigit:]-]+$ && "$team" =~ ^[A-Z0-9]+$ && "$app_id" == "$team.$bundle_id" ]]
keychain_password="$(openssl rand -hex 32)"
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$signing_dir/certificate.p12" -P "$IOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -t cert -f pkcs12 -k "$keychain"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain"

profiles="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
mkdir -p "$profiles"
installed_profile="$profiles/$profile_uuid.mobileprovision"
cp "$signing_dir/app.mobileprovision" "$installed_profile"

archive="$RUNNER_TEMP/OfferingRecorderSlint-signed.xcarchive"
xcodebuild -project OfferingRecorderSlint.xcodeproj -scheme OfferingRecorderSlint \
    -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
    -derivedDataPath "$RUNNER_TEMP/slint-signed-build" -archivePath "$archive" \
    ARCHS=arm64 CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM="$team" \
    CODE_SIGN_IDENTITY="Apple Distribution" PROVISIONING_PROFILE_SPECIFIER="$profile_uuid" \
    CURRENT_PROJECT_VERSION="$GITHUB_RUN_NUMBER.$GITHUB_RUN_ATTEMPT" archive

python3 - "$signing_dir/export.plist" "$team" "$bundle_id" "$profile_uuid" <<'PYTHON'
import plistlib
import sys

path, team, bundle, profile = sys.argv[1:]
with open(path, "wb") as file:
    plistlib.dump({
        "method": "app-store-connect",
        "destination": "export",
        "signingStyle": "manual",
        "signingCertificate": "Apple Distribution",
        "teamID": team,
        "provisioningProfiles": {bundle: profile},
    }, file)
PYTHON

xcodebuild -exportArchive -archivePath "$archive" \
    -exportOptionsPlist "$signing_dir/export.plist" -exportPath "$RUNNER_TEMP/slint-ios-export"
