// MakOS platform defaults. Keep startup local; page navigation still uses
// Firefox's normal Necko/NSS stack and remains fully network-enabled.
pref("browser.search.region", "AU");
pref("browser.region.network.url", "");
pref("browser.region.update.enabled", false);
pref("network.captive-portal-service.enabled", false);
pref("network.connectivity-service.enabled", false);
// MakOS Necko transport currently exposes IPv4 sockets. Prevent an IPv6
// candidate from aborting an otherwise healthy IPv4 HTTPS connection with
// NS_ERROR_SOCKET_ADDRESS_NOT_SUPPORTED.
pref("network.dns.disableIPv6", true);
pref("services.settings.server", "data:,#remote-settings-disabled");
pref("browser.startup.homepage", "about:blank");
pref("browser.newtabpage.enabled", false);
pref("browser.newtab.preload", false);
pref("browser.newtabpage.activity-stream.feeds.section.topstories", false);
pref("browser.newtabpage.activity-stream.feeds.topsites", false);
pref("app.update.auto", false);
pref("app.update.enabled", false);
pref("extensions.update.enabled", false);
pref("extensions.systemAddon.update.enabled", false);
pref("extensions.getAddons.cache.enabled", false);
pref("extensions.blocklist.enabled", false);
pref("security.remote_settings.crlite_filters.enabled", false);
pref("security.remote_settings.intermediates.enabled", false);
pref("toolkit.telemetry.enabled", false);
pref("datareporting.healthreport.uploadEnabled", false);
