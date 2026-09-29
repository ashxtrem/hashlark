# Hashlark user guide

Hashlark searches many torrent indexers at once and hands what you pick to your torrent client. It doesn't download anything itself, hosts no content and sends no telemetry.

## Install

- **Windows:** run `Hashlark_x.y.z_x64-setup.exe` (installs for your user only) or the `.msi`.
- **macOS:** open the `.dmg` and drag Hashlark to Applications.
- **Linux:** use the `.AppImage` (make it executable), or install the `.deb`.
- **Server / NAS:** see [deploy/README.md](deploy/README.md).

You also need a torrent client that opens magnet links, for example qBittorrent, Transmission or Deluge. Hashlark tells you if none is set up.

## Searching

1. Type what you're looking for and press **Enter**. Press **/** anywhere to jump to the search box.
2. Optionally pick **categories** (Movies, TV, Software, …). Only providers with those categories are searched.
3. Results appear as each provider answers:
   - the chips above the list show each provider's status, result count and speed;
   - the same torrent found on several sites is shown once, listing every source.
4. Sort by best match, seeders, size or date.

On each result:

| Button | What it does |
|---|---|
| **⬇ Open in client** | Sends the magnet (or `.torrent`) to your torrent client. |
| **Copy magnet** | Copies the magnet link. |
| **.torrent** | Saves the `.torrent` file to your Downloads folder (or the folder in Settings) and opens it. |
| **★** | Saves the result to **Favourites**. |

Click a row for details: infohash, publish date, every site that has it, and a link to the site's page.

## Providers

**Providers** lists every indexer, with its health:

- *Healthy*, *Unreliable* or *Failing*, plus its success rate and typical response time;
- *Paused*: the provider failed five times in a row. It's skipped for 15 minutes, then 1 hour, then 6 hours, and retried automatically. Turning it off and on again un-pauses it straight away.

Use **Test** to try a provider now. **Details** shows its settings, network route and definition.

Hashlark starts with legal sources only: the Internet Archive, Linuxtracker, Academic Torrents and FOSS Torrents. To add more, use **Add provider**:

- **Definition file:** paste or load a `.yml` definition, check it, preview a search, then add it. Writing definitions is covered in [definition-format.md](definition-format.md).
- **Jackett / Prowlarr:** enter the Torznab URL and API key to search every indexer configured there, including private trackers.
- **Definition repositories** (at the bottom of the page): add a repository URL to get a curated set of definitions that update themselves daily.
  - Repositories are signed. Hashlark remembers the key and refuses updates signed with a different one.
  - New providers from a repository start **disabled**, so you can choose which to use.

Providers that need an account show their settings under **Details**. Passwords and API keys are kept in your system keychain.

## When a site blocks you

A provider's status explains what went wrong:

- **Blocked:** the site couldn't be reached. This often means your network or country blocks it. Try, in order:
  1. **Settings → Network & privacy → Encrypted DNS.** This is on by default, and gets past most DNS-based blocks.
  2. A **proxy** (HTTP or SOCKS5).
  3. **Tor**: install and start Tor (the Tor service, Tor Browser or Orbot), then turn on **Route everything through Tor**, or route just one provider through Tor under **Details → Network**. Tor is slower but reaches most blocked sites, and definitions can list `.onion` mirrors.
- **Needs browser check:** the site shows a "checking your browser" page. Click **Open site to continue**, complete the check in the window that opens, then click **Done**. Hashlark keeps that site's cookies for this provider until they expire. (Headless servers can't do this; use Jackett/Prowlarr with FlareSolverr for such sites.)
- **Site changed:** the site's layout changed and its definition needs an update. Repository definitions usually get fixed within a day.
- **Login failed:** check the provider's username, password or API key.

## Favourites and history

- **Favourites** keeps results you starred, and they still open later.
- **History** lists recent searches; click one to run it again. You can turn history off, or clear it, in **Settings**.

## Settings

| Section | What's there |
|---|---|
| Search | Timeout per provider, how many providers run at once, result caching, history. |
| Network & privacy | Encrypted DNS provider, proxy, Tor, requests per second per site. |
| Magnet links | Add public trackers to every magnet, and a tracker-list URL that's refreshed daily. |
| Downloads | Folder for `.torrent` files. |
| Appearance & updates | Light, dark or system theme; automatic update checks. |

## Android

Download the APK for your phone from the [releases page](https://github.com/ashxtrem/hashlark/releases): `arm64-v8a` fits nearly every phone (including the Galaxy Z Fold7); the `universal` APK works everywhere and is larger. Open it and allow your browser or file manager to install apps. Check the APK against `SHA256SUMS` and the signing-certificate fingerprint in the release notes if you like. Hashlark needs Android 8.0 or newer and a torrent client that opens magnet links.

Everything above works the same on Android. What is different:

- **Layouts follow the window.** On a phone or a foldable's cover screen you get one screen at a time (bottom bar; tap a result to see its details, Back returns to the list). On a tablet or an unfolded foldable the results and details sit side by side, and on very wide windows a filter panel joins them and the results become a sortable table. Half-open a foldable and Hashlark uses both halves: with the fold **horizontal** (tabletop), results are on top and the search box, chips and keyboard below; with the fold **vertical** (book), the list is on one side and the details on the other. Folding, unfolding or rotating in the middle of a search keeps the results.
- **Provider status is one chip** next to the result count ("4 providers", or "3 of 4 providers" in red when some failed) so the results get the room. Tap it for each provider's count and speed, and for the way forward when a site needs a browser check.
- **Search from anywhere:** share text or an IMDb link to Hashlark, or select text in any app and choose *Search in Hashlark*. Long-press the app icon for *New search*, *Favourites*, *History* and your last searches.
- **Torrent client:** magnets open in the client Android picks; choose one in **Settings, Magnets and downloads**. `.torrent` files are saved to your Downloads folder and can be opened from the message that appears.
- **Adding providers:** open a `.yml` definition from a file manager, or a `hashlark://repo?url=...` link. Hashlark always shows what it found and asks before adding. For a repository it shows the signing key first; only accept keys of people you trust.
- **Browser checks** open in a window inside the app: pass the check, then tap **Done**.
- **Tor:** built-in Tor works on Android and starts only when needed; on networks that block Tor, install Orbot (which supports bridges), start it, and choose *Orbot or my own Tor* in Settings.
- **Background sync:** repositories and the tracker list refresh about once a day, only on Wi-Fi and when the battery is not low.
- **Keyboard and mouse** (tablets, DeX, Chromebooks): `Ctrl+F` focuses the search box, arrow keys move through results, `Enter` opens, `Ctrl+C` copies the selected magnet, right-click opens the result menu. In split screen you can drag a result into a torrent client.
- **Privacy:** nothing is backed up, and passwords and API keys are encrypted with a key stored in the Android Keystore.

## Privacy

Hashlark sends no analytics or crash reports. It only connects to:

- the providers you enabled;
- definition repositories you added;
- your tracker-list URL, if you set one;
- GitHub, to check for updates (you can turn this off).

Your searches, favourites and settings stay in a database on your computer.

Hashlark is a neutral search tool. You are responsible for complying with the law and copyright where you live.
