package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.entity.Player;
import org.bukkit.event.*;
import org.bukkit.event.inventory.*;
import org.bukkit.inventory.*;
import org.bukkit.inventory.meta.*;
import java.util.*;
import java.util.function.Consumer;

public final class AreaMenus implements Listener {
    static final List<Integer> FLAG_SLOTS = List.of(10,11,12,13,14,15,16,19,20,21,22,23,24,25,28,29,30,31,32,33,34,37);
    static final List<Integer> MEMBER_SLOTS = List.of(10,11,12,13,14,15,16,19,20,21,22,23,24,25,28,29,30,31,32,33,34,37,38,39,40,41,42,43);
    static final String HOME_TITLE = "领地核心", FLAGS_TITLE = "领地核心 > 领地设置", MEMBERS_TITLE = "领地核心 > 成员管理";
    public static final class Menu implements InventoryHolder {
        private final Inventory inventory;
        private final Map<Integer, Consumer<ClickType>> actions = new HashMap<>();
        private final Area area;
        private Menu(String title, int size, Area area) { this.area = area; inventory = Bukkit.createInventory(this,size,title); }
        @Override public Inventory getInventory() { return inventory; }
        private void set(int slot, Material material, String title, List<String> lore, Consumer<ClickType> action) {
            ItemStack item = new ItemStack(material); ItemMeta meta = item.getItemMeta();
            meta.setDisplayName(CoreItems.color(title)); meta.setLore(lore.stream().map(CoreItems::color).toList()); item.setItemMeta(meta);
            inventory.setItem(slot,item); if (action != null) actions.put(slot,action);
        }
    }
    private final AreaService s;
    public AreaMenus(AreaService s) { this.s = s; }
    public void home(Player p, Area a) {
        s.requireManager(p,a); Menu m = new Menu(HOME_TITLE,9,a);
        m.set(4,Material.BOOK,"&f领地核心",List.of("&7左键: 领地设置","&7右键: 成员管理"),click -> {
            if (click.isLeftClick()) flags(p,a,null); else if (click.isRightClick()) members(p,a,0);
        });
        p.openInventory(m.inventory);
    }
    List<Flag> visibleFlags(boolean member) {
        List<String> locked = s.plugin.getConfig().getStringList("Settings.IgnoreFlags");
        return Arrays.stream(Flag.values()).filter(f -> (!member || f.personal) && !locked.contains(f.id)).toList();
    }
    static int page(int requested, int count, int capacity) { return Math.max(0,Math.min(requested,Math.max(0,(count-1)/capacity))); }
    public void flags(Player p, Area a, String memberKey) { flags(p,a,memberKey,0); }
    void flags(Player p, Area a, String memberKey, int requested) {
        s.requireManager(p,a);
        Area.Member member = memberKey == null ? null : a.members.get(memberKey);
        if (memberKey != null && member == null) throw new IllegalArgumentException("成员已被删除");
        List<Flag> flags = visibleFlags(member != null); int page = page(requested,flags.size(),FLAG_SLOTS.size());
        Menu m = new Menu(member == null ? FLAGS_TITLE : "领地核心 > 成员权限",54,a);
        int start = page*FLAG_SLOTS.size();
        for (int i = start; i < Math.min(start+FLAG_SLOTS.size(),flags.size()); i++) {
            Flag f = flags.get(i);
            boolean enabled = member == null ? s.enabled(a,f) : member.flags.getOrDefault(f,s.defaultValue(f));
            m.set(FLAG_SLOTS.get(i-start),enabled ? Material.GREEN_TERRACOTTA : Material.RED_TERRACOTTA,"&f"+f.title,
                    List.of("&7"+f.id,enabled ? "&a允许" : "&c禁止","&7左键切换，右键恢复默认"),click -> {
                if (!click.isLeftClick() && !click.isRightClick()) return;
                s.requireManager(p,a); s.mutableFlag(f,memberKey != null);
                Map<Flag,Boolean> values;
                if (memberKey != null) {
                    Area.Member current = a.members.get(memberKey);
                    if (current == null) throw new IllegalArgumentException("成员已被删除"); values = current.flags;
                } else values = a.flags;
                s.change(() -> {
                    if (click.isRightClick()) { if (memberKey != null) values.remove(f); else values.put(f,s.defaultValue(f)); }
                    else values.put(f,!(memberKey == null ? s.enabled(a,f) : values.getOrDefault(f,s.defaultValue(f))));
                });
                flags(p,a,memberKey,page);
            });
        }
        if (page > 0) m.set(45,Material.ARROW,"&f上一页",List.of(),click -> flags(p,a,memberKey,page-1));
        m.set(49,Material.REDSTONE_BLOCK,"&f返回",List.of(),click -> { if (memberKey == null) home(p,a); else members(p,a,0); });
        if (start+FLAG_SLOTS.size() < flags.size()) m.set(53,Material.ARROW,"&f下一页",List.of(),click -> flags(p,a,memberKey,page+1));
        if (memberKey != null) m.set(48,Material.BARRIER,"&c移除成员",List.of("&7Shift + 点击确认"),click -> {
            s.requireManager(p,a); if (!click.isShiftClick()) return;
            s.change(() -> a.members.remove(memberKey)); members(p,a,0);
        });
        p.openInventory(m.inventory);
    }
    public void members(Player p, Area a, int requested) {
        s.requireManager(p,a); Menu m = new Menu(MEMBERS_TITLE,54,a);
        var members = new ArrayList<>(a.members.entrySet()); int page = page(requested,members.size(),MEMBER_SLOTS.size()), start = page*MEMBER_SLOTS.size();
        for (int i = start; i < Math.min(start+MEMBER_SLOTS.size(),members.size()); i++) {
            var member = members.get(i); int slot = MEMBER_SLOTS.get(i-start);
            m.set(slot,Material.PLAYER_HEAD,"&f"+member.getValue().name,List.of("&7点击修改独立权限"),click -> flags(p,a,member.getKey()));
            ItemStack head = m.inventory.getItem(slot);
            if (head != null && head.getItemMeta() instanceof SkullMeta meta) {
                var known = Bukkit.getOfflinePlayerIfCached(member.getValue().name);
                if (known != null) { meta.setOwningPlayer(known); head.setItemMeta(meta); }
            }
        }
        if (page > 0) m.set(45,Material.ARROW,"&f上一页",List.of(),click -> members(p,a,page-1));
        m.set(49,Material.OAK_SIGN,"&f添加成员",List.of("&7点击添加领地成员"),click -> s.memberInput.begin(p,a));
        m.set(48,Material.REDSTONE_BLOCK,"&f返回",List.of(),click -> home(p,a));
        if (start+MEMBER_SLOTS.size() < members.size()) m.set(53,Material.ARROW,"&f下一页",List.of(),click -> members(p,a,page+1));
        p.openInventory(m.inventory);
    }
    public void list(Player p, int requested) {
        Menu m = new Menu("领地列表",54,null);
        var areas = s.index.all().stream().filter(a -> s.manages(p,a)).toList(); int page = page(requested,areas.size(),45), start = page*45;
        for (int i = start; i < Math.min(start+45,areas.size()); i++) {
            Area a = areas.get(i); Volume v = a.volumes.getFirst();
            m.set(i-start,Material.BEACON,"&e"+a.name,List.of("&7主人："+a.ownerName,"&7"+a.world+" "+v.x()+", "+v.y()+", "+v.z()),click -> home(p,a));
        }
        if (page > 0) m.set(45,Material.ARROW,"&f上一页",List.of(),click -> list(p,page-1));
        if (start+45 < areas.size()) m.set(53,Material.ARROW,"&f下一页",List.of(),click -> list(p,page+1));
        p.openInventory(m.inventory);
    }
    @EventHandler(priority = EventPriority.HIGHEST)
    public void click(InventoryClickEvent e) {
        if (!(e.getView().getTopInventory().getHolder() instanceof Menu m)) return;
        e.setCancelled(true);
        if (!(e.getWhoClicked() instanceof Player p) || e.getRawSlot() < 0 || e.getRawSlot() >= m.inventory.getSize()) return;
        Consumer<ClickType> action = m.actions.get(e.getRawSlot()); if (action == null) return;
        ClickType click = e.getClick();
        Bukkit.getScheduler().runTask(s.plugin,() -> {
            if (!p.isOnline() || p.getOpenInventory().getTopInventory() != m.inventory) return;
            try { s.checkHealthy(); if (m.area != null) s.requireManager(p,m.area); action.accept(click); }
            catch (IllegalArgumentException ex) { p.closeInventory(); AreaService.message(p,ex.getMessage()); }
        });
    }
    @EventHandler(priority = EventPriority.HIGHEST)
    public void drag(InventoryDragEvent e) { if (e.getView().getTopInventory().getHolder() instanceof Menu) e.setCancelled(true); }
}
