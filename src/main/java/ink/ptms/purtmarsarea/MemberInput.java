package ink.ptms.purtmarsarea;

import io.papermc.paper.event.player.AsyncChatEvent;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.bukkit.Bukkit;
import org.bukkit.entity.Player;
import org.bukkit.event.*;
import org.bukkit.event.player.*;
import org.bukkit.event.inventory.InventoryOpenEvent;
import org.bukkit.scheduler.BukkitTask;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicBoolean;

/** Chat only captures immutable input off-thread. Every game-state check runs on the server thread. */
public final class MemberInput implements Listener {
    private record Key(Player player) {
        @Override public int hashCode() { return System.identityHashCode(player); }
        @Override public boolean equals(Object other) { return other instanceof Key k && player == k.player; }
    }
    private static final class Session {
        final UUID player, world;
        final Area area, standingIn;
        final AtomicBoolean claimed = new AtomicBoolean();
        BukkitTask timeout;
        Session(Player p, Area area, Area standingIn) {
            player = p.getUniqueId(); world = p.getWorld().getUID(); this.area = area; this.standingIn = standingIn;
        }
    }
    private final AreaService s;
    private final Map<Key,Session> sessions = new ConcurrentHashMap<>();
    public MemberInput(AreaService s) { this.s = s; }
    public void begin(Player p, Area a) {
        s.requireManager(p,a); cancel(p); p.closeInventory();
        Key key = new Key(p); Session session = new Session(p,a,s.at(p.getLocation()));
        sessions.put(key,session);
        int seconds = Math.max(5,Math.min(300,s.plugin.getConfig().getInt("Settings.MemberInputTimeout",60)));
        session.timeout = Bukkit.getScheduler().runTaskLater(s.plugin,() -> {
            if (remove(key,session)) AreaService.message(p,"添加成员已超时，请重新打开菜单");
        },seconds*20L);
        AreaService.message(p,"请直接在聊天中输入玩家名称（仅自己可见），输入 取消 可退出；"+seconds+" 秒内有效");
    }
    private boolean remove(Key key, Session session) {
        if (!sessions.remove(key,session)) return false;
        if (session.timeout != null) session.timeout.cancel();
        return true;
    }
    public void cancel(Player p) {
        Key key = new Key(p); Session session = sessions.get(key);
        if (session != null) remove(key,session);
    }
    public void clear() {
        sessions.values().forEach(session -> { if (session.timeout != null) session.timeout.cancel(); });
        sessions.clear();
    }
    @EventHandler(priority = EventPriority.LOWEST)
    public void chat(AsyncChatEvent e) {
        Key key = new Key(e.getPlayer()); Session session = sessions.get(key);
        if (session == null) return;
        e.setCancelled(true); e.viewers().clear();
        String input = PlainTextComponentSerializer.plainText().serialize(e.message()).trim();
        if (!session.claimed.compareAndSet(false,true)) return;
        Bukkit.getScheduler().runTask(s.plugin,() -> accept(key,session,input));
    }
    private void accept(Key key, Session session, String input) {
        if (!remove(key,session)) return;
        Player p = key.player;
        if (!p.isOnline() || !p.getUniqueId().equals(session.player)) return;
        if (input.equalsIgnoreCase("cancel") || input.equals("取消")) {
            AreaService.message(p,"已取消添加成员"); return;
        }
        try {
            if (!p.getWorld().getUID().equals(session.world) || s.at(p.getLocation()) != session.standingIn)
                throw new IllegalArgumentException("位置或领地已发生变化，请重新打开菜单");
            s.requireManager(p,session.area);
            s.addMember(p,session.area,input);
            AreaService.message(p,"已添加领地成员："+input);
        } catch (IllegalArgumentException ex) { AreaService.message(p,ex.getMessage()); }
    }
    @EventHandler public void quit(PlayerQuitEvent e) { cancel(e.getPlayer()); }
    @EventHandler public void world(PlayerChangedWorldEvent e) { cancel(e.getPlayer()); }
    @EventHandler public void open(InventoryOpenEvent e) { if (e.getPlayer() instanceof Player p) cancel(p); }
}
