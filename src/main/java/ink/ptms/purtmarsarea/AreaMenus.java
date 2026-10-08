package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.entity.Player;
import org.bukkit.event.*;
import org.bukkit.event.inventory.*;
import org.bukkit.inventory.*;
import org.bukkit.inventory.meta.ItemMeta;
import java.util.*;
import java.util.function.Consumer;

public final class AreaMenus implements Listener {
    public static final class Menu implements InventoryHolder {
        private final Inventory inventory;
        private final Map<Integer, Consumer<InventoryClickEvent>> actions = new HashMap<>();
        private final UUID area;
        private Menu(String title, UUID area) { this.area = area; inventory = Bukkit.createInventory(this, 54, title); }
        @Override public Inventory getInventory() { return inventory; }
        private void set(int slot, Material material, String title, List<String> lore, Consumer<InventoryClickEvent> action) {
            ItemStack item = new ItemStack(material); ItemMeta meta = item.getItemMeta();
            meta.setDisplayName(CoreItems.color(title)); meta.setLore(lore.stream().map(CoreItems::color).toList()); item.setItemMeta(meta);
            inventory.setItem(slot, item); if (action != null) actions.put(slot, action);
        }
    }
    private final AreaService s;
    public AreaMenus(AreaService s) { this.s = s; }
    public void home(Player p, Area a) {
        s.requireManager(p, a); Menu m = new Menu("领地核心 · " + shorten(a.name), a.id);
        m.set(13, Material.BEACON, "&e" + a.name, List.of("&7主人：" + a.ownerName, "&7成员：" + a.members.size(), "&7扩展：" + (a.volumes.size()-1), "&7ID：" + a.id.toString().substring(0,8)), null);
        m.set(29, Material.COMPARATOR, "&e领地权限", List.of("&7管理访客与环境规则"), e -> flags(p, a, null));
        m.set(31, Material.PLAYER_HEAD, "&e成员管理", List.of("&7成员独立权限", "&7添加：/pa add 玩家名"), e -> members(p, a, 0));
        m.set(33, Material.OAK_SIGN, "&e名称与提示", List.of("&7/pa name 名称", "&7/pa welcome 欢迎内容", "&7/pa farewell 离开内容", "&7使用 {name} 引用领地名", "&7内容填 off 可关闭提示"), null);
        p.openInventory(m.inventory);
    }
    public void flags(Player p, Area a, String memberKey) {
        s.requireManager(p, a);
        Area.Member member = memberKey == null ? null : a.members.get(memberKey);
        if (memberKey != null && member == null) throw new IllegalArgumentException("成员已被删除");
        Menu m = new Menu(member == null ? "领地权限" : "成员权限 · " + member.name, a.id);
        int slot = 0;
        for (Flag f : Flag.values()) {
            if (member != null && !f.personal) continue;
            boolean locked = s.plugin.getConfig().getStringList("Settings.IgnoreFlags").contains(f.id);
            boolean enabled = member == null ? s.enabled(a, f) : member.flags.getOrDefault(f, s.defaultValue(f));
            m.set(slot++, enabled ? Material.LIME_DYE : Material.GRAY_DYE, "&f" + f.title,
                    List.of("&7" + f.id, enabled ? "&a允许" : "&c禁止", locked ? "&8服务器已锁定" : "&7左键切换，右键恢复默认"), e -> {
                        s.requireManager(p, a); s.mutableFlag(f, memberKey != null);
                        Map<Flag, Boolean> values;
                        if (memberKey != null) {
                            Area.Member current = a.members.get(memberKey); if (current == null) throw new IllegalArgumentException("成员已被删除"); values = current.flags;
                        } else values = a.flags;
                        s.change(() -> {
                            if (e.isRightClick()) {
                                if (memberKey != null) values.remove(f);
                                else values.put(f, s.plugin.getConfig().getBoolean("Settings.Flags." + f.id, f.defaultValue));
                            } else values.put(f, !(memberKey == null ? s.enabled(a, f) : values.getOrDefault(f, s.defaultValue(f))));
                        });
                        flags(p, a, memberKey);
                    });
        }
        m.set(45, Material.ARROW, "&e返回", List.of(), e -> { if (memberKey == null) home(p, a); else members(p, a, 0); });
        if (memberKey != null) m.set(49, Material.BARRIER, "&c移除成员", List.of("&7Shift + 点击确认"), e -> {
            s.requireManager(p, a); if (!e.isShiftClick()) return;
            s.change(() -> a.members.remove(memberKey)); members(p, a, 0);
        });
        p.openInventory(m.inventory);
    }
    public void members(Player p, Area a, int page) {
        s.requireManager(p, a); Menu m = new Menu("领地成员 · 第 " + (page+1) + " 页", a.id);
        var members = new ArrayList<>(a.members.entrySet()); int start = page*45;
        for (int i = start; i < Math.min(start+45, members.size()); i++) {
            var member = members.get(i);
            m.set(i-start, Material.PLAYER_HEAD, "&e" + member.getValue().name, List.of("&7点击修改独立权限"), e -> flags(p, a, member.getKey()));
        }
        if (page > 0) m.set(45, Material.ARROW, "&e上一页", List.of(), e -> members(p, a, page-1));
        m.set(49, Material.OAK_SIGN, "&e添加成员", List.of("&7使用 /pa add 玩家名", "&7成员默认仍无法建筑，需单独授权"), e -> {
            p.closeInventory(); AreaService.message(p, "请站在领地内使用 /pa add 玩家名");
        });
        m.set(48, Material.BARRIER, "&e返回", List.of(), e -> home(p, a));
        if (start+45 < members.size()) m.set(53, Material.ARROW, "&e下一页", List.of(), e -> members(p, a, page+1));
        p.openInventory(m.inventory);
    }
    public void list(Player p, int page) {
        Menu m = new Menu("领地列表 · 第 " + (page+1) + " 页", null);
        var areas = s.index.all().stream().filter(a -> s.manages(p, a)).toList(); int start = page*45;
        for (int i = start; i < Math.min(start+45, areas.size()); i++) {
            Area a = areas.get(i); Volume v = a.volumes.getFirst();
            m.set(i-start, Material.BEACON, "&e" + a.name, List.of("&7主人：" + a.ownerName, "&7" + a.world + " " + v.x()+", "+v.y()+", "+v.z()), e -> home(p, a));
        }
        if (page > 0) m.set(45, Material.ARROW, "&e上一页", List.of(), e -> list(p, page-1));
        if (start+45 < areas.size()) m.set(53, Material.ARROW, "&e下一页", List.of(), e -> list(p, page+1));
        p.openInventory(m.inventory);
    }
    @EventHandler(priority = EventPriority.HIGHEST)
    public void click(InventoryClickEvent e) {
        if (!(e.getView().getTopInventory().getHolder() instanceof Menu m)) return;
        e.setCancelled(true); // Includes shift, number keys, offhand swaps and double clicks.
        if (!(e.getWhoClicked() instanceof Player p) || e.getRawSlot() < 0 || e.getRawSlot() >= 54) return;
        Consumer<InventoryClickEvent> action = m.actions.get(e.getRawSlot()); if (action == null) return;
        // Inventory transitions must happen after this inventory event has completed.
        Bukkit.getScheduler().runTask(s.plugin, () -> {
            if (!p.isOnline() || p.getOpenInventory().getTopInventory() != m.inventory) return;
            try {
                s.checkHealthy();
                if (m.area != null) {
                    Area a = s.index.get(m.area); if (a == null) throw new IllegalArgumentException("领地已不存在"); s.requireManager(p, a);
                }
                action.accept(e);
            } catch (IllegalArgumentException ex) { p.closeInventory(); AreaService.message(p, ex.getMessage()); }
        });
    }
    @EventHandler(priority = EventPriority.HIGHEST)
    public void drag(InventoryDragEvent e) { if (e.getView().getTopInventory().getHolder() instanceof Menu) e.setCancelled(true); }
    private String shorten(String text) { return text.length() > 20 ? text.substring(0,20) : text; }
}
