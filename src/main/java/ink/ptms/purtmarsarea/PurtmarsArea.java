package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.plugin.java.JavaPlugin;
import taboolib.common.platform.Plugin;
import taboolib.platform.BukkitPlugin;
import java.util.*;

/** TabooLib owns bootstrap, dependency isolation and the plugin lifecycle. */
public final class PurtmarsArea extends Plugin {
    private AreaService service;
    @Override public void onEnable() {
        JavaPlugin host = BukkitPlugin.getInstance(); host.saveDefaultConfig();
        service = new AreaService(host);
        service.reload();
        AreaCommands commands = new AreaCommands(service);
        Objects.requireNonNull(host.getCommand("purtmarsarea")).setExecutor(commands);
        host.getCommand("purtmarsarea").setTabCompleter(commands);
        Bukkit.getPluginManager().registerEvents(new ProtectionListener(service), host);
        Bukkit.getPluginManager().registerEvents(service.menus, host);
        Bukkit.getPluginManager().registerEvents(service.memberInput, host);
        Bukkit.getScheduler().runTaskTimer(host, service::particles, 20, 20);
        host.getLogger().info("PurtmarsArea 2.1 | Paper 26.2 | TabooLib 6.3.0-0e3a911");
    }
    @Override public void onDisable() { if (service != null) { service.close(); if (service.healthy) service.save(); } }
}
