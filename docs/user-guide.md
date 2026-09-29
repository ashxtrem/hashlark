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

## Privacy

Hashlark sends no analytics or crash reports. It only connects to:

- the providers you enabled;
- definition repositories you added;
- your tracker-list URL, if you set one;
- GitHub, to check for updates (you can turn this off).

Your searches, favourites and settings stay in a database on your computer.

Hashlark is a neutral search tool. You are responsible for complying with the law and copyright where you live.
