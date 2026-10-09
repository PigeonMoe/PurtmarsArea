package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.entity.Player;
import org.bukkit.plugin.java.JavaPlugin;
import java.util.*;
import java.util.logging.Level;

public final class AreaService {
    public final JavaPlugin plugin;
    public final AreaStore store;
    public final CoreItems cores;
    public final AreaMenus menus;
    public final MemberInput memberInput;
    private final Map<UUID,OccupationNotice> occupations = new HashMap<>();
    private record OccupationNotice(UUID area, long at) {}
    public AreaIndex index = new AreaIndex();
    public boolean healthy;
    public final EnumMap<Flag, Long> denials = new EnumMap<>(Flag.class);
    private final Map<UUID, Long> notices = new HashMap<>();
    private long particleTick;
    private final Deque<String> explosionReports = new ArrayDeque<>();
    public List<String> explosionReports() { return List.copyOf(explosionReports); }
    public void recordExplosion(String source, Location origin, int affected, int denied, Location first) {
        String time = java.time.LocalTime.now().withNano(0).toString();
        String text = time+" | "+source+" | "+coordinates(origin)+" | 候选方块 "+affected+" | 受保护 "+denied
                + (denied > 0 ? " | 已取消整批方块破坏 | 首个命中 "+coordinates(first) : " | 未命中保护范围");
        explosionReports.addFirst(text); while (explosionReports.size() > 10) explosionReports.removeLast();
    }
    private static String coordinates(Location l) {
        return l == null || l.getWorld() == null ? "未知位置" : l.getWorld().getName()+" "+l.getBlockX()+","+l.getBlockY()+","+l.getBlockZ();
    }
    public AreaService(JavaPlugin plugin) {
        this(plugin, new CoreItems(plugin));
    }
    AreaService(JavaPlugin plugin, CoreItems cores) {
        this.plugin = plugin; store = new AreaStore(plugin.getDataFolder().toPath().resolve("areas.yml"));
        this.cores = cores; memberInput = new MemberInput(this); menus = new AreaMenus(this);
    }
    public void reload() {
        memberInput.clear(); occupations.clear();
        try {
            plugin.reloadConfig(); AreaIndex loaded = store.load(); cores.validate();
            index = loaded; healthy = true;
            plugin.getLogger().info("已加载 " + index.all().size() + " 个领地");
        } catch (Exception e) {
            healthy = false;
            plugin.getLogger().log(Level.SEVERE, "领地加载失败，进入保护锁定模式；修复文件后 /pa reload，原数据不会覆盖", e);
        }
    }
    public void checkHealthy() { if (!healthy) throw new IllegalArgumentException("领地系统已锁定，请联系管理员修复配置或数据"); }
    public void change(Runnable action) {
        checkHealthy();
        try { action.run(); store.save(index); }
        catch (Exception e) {
            try { index = store.load(); } catch (Exception restore) { healthy = false; e.addSuppressed(restore); }
            plugin.getLogger().log(Level.SEVERE, "领地修改失败，已回读磁盘数据", e);
            throw new IllegalArgumentException("修改未完成：" + e.getMessage());
        }
    }
    public void save() { checkHealthy(); change(() -> {}); }
    public Area at(Location l) { return l.getWorld() == null ? null : index.at(l.getWorld().getName(), l.getBlockX(), l.getBlockY(), l.getBlockZ()); }
    public Area core(Location l) {
        Area a = at(l);
        return a != null && a.volumes.stream().anyMatch(v -> v.isCore(l.getBlockX(), l.getBlockY(), l.getBlockZ())) ? a : null;
    }
    public boolean admin(org.bukkit.command.CommandSender sender) { return sender.hasPermission("PurtmarsArea.command.admin"); }
    public boolean bypass(Player p) { return p.hasPermission("PurtmarsArea.bypass"); }
    public boolean manages(Player p, Area a) { return a.isOwner(p.getUniqueId(), p.getName()) || admin(p); }
    public void requireManager(Player p, Area a) {
        checkHealthy(); if (index.get(a.id) != a) throw new IllegalArgumentException("领地已发生变化，请重新打开菜单");
        if (!manages(p, a)) throw new IllegalArgumentException("只有领地主人或管理员可以管理");
    }
    public boolean allowed(Player p, Location l, Flag f) {
        if (!healthy) { deny(p, f); return false; }
        if (bypass(p)) return true;
        Area a = at(l);
        boolean allowed = a == null ? !plugin.getConfig().getStringList("Settings.ClaimWorld." + l.getWorld().getName()).contains(f.id)
                : a.allows(f, p.getUniqueId(), p.getName(), defaultValue(f));
        if (!allowed) deny(p, f);
        return allowed;
    }
    public boolean defaultValue(Flag flag) { return plugin.getConfig().getBoolean("Settings.Flags." + flag.id, flag.defaultValue); }
    public boolean enabled(Area area, Flag flag) { return area.flags.getOrDefault(flag, defaultValue(flag)); }
    public boolean natural(Location l, Flag flag) { Area a = at(l); return healthy && (a == null || enabled(a, flag)); }
    public void deny(Player p, Flag f) {
        denials.merge(f, 1L, Long::sum);
        long now = System.currentTimeMillis();
        if (now - notices.getOrDefault(p.getUniqueId(), 0L) > 1000) {
            notices.put(p.getUniqueId(), now); message(p, healthy ? "此处不允许：" + f.title : "领地系统已锁定，请联系管理员");
        }
    }
    public void quit(Player p) { notices.remove(p.getUniqueId()); occupations.remove(p.getUniqueId()); memberInput.cancel(p); }
    public void close() { memberInput.clear(); occupations.clear(); }
    public void showOccupation(Player p, Area a) {
        long now = System.currentTimeMillis(); OccupationNotice previous = occupations.get(p.getUniqueId());
        if (previous != null && previous.area.equals(a.id) && now-previous.at < 2000) return;
        occupations.put(p.getUniqueId(),new OccupationNotice(a.id,now));
        p.sendTitle("§6私人领地",occupationSubtitle(a.ownerName),10,40,10);
    }
    static String occupationSubtitle(String owner) { return "§7已被 §f"+owner+"§7 占领."; }
    public void mutableFlag(Flag f, boolean member) {
        if (plugin.getConfig().getStringList("Settings.IgnoreFlags").contains(f.id)) throw new IllegalArgumentException("管理员已锁定该权限");
        if (member && !f.personal) throw new IllegalArgumentException("环境权限不能单独设置给成员");
    }
    public void create(Player p, Location l, String id) {
        checkHealthy();
        if (!p.hasPermission("PurtmarsArea.create")) throw new IllegalArgumentException("没有创建领地的权限");
        if (!plugin.getConfig().getStringList("Settings.EnableWorld").contains(l.getWorld().getName())
                && !p.hasPermission("PurtmarsArea.bypass." + l.getWorld().getName())) throw new IllegalArgumentException("该世界不允许放置领地核心");
        Volume v = cores.volume(id, l); Set<Area> overlaps = index.overlaps(l.getWorld().getName(), v);
        if (!overlaps.isEmpty()) throw new IllegalArgumentException("核心保护范围不能与任何已有核心重叠，包括自己的领地");
        long count = index.all().stream().filter(a -> a.isOwner(p.getUniqueId(),p.getName())).count();
        if (!admin(p) && count >= plugin.getConfig().getInt("Settings.AreaLimit",10)) throw new IllegalArgumentException("领地数量达到上限");
        Area a = new Area(UUID.randomUUID(),l.getWorld().getName(),p.getUniqueId(),p.getName(),v);
        for (Flag f : Flag.values()) a.flags.put(f,defaultValue(f));
        change(() -> index.add(a)); message(p,"领地已创建，右键核心进行管理");
    }
    public void breakCore(Player p, Location l) {
        Area a = Objects.requireNonNull(core(l)); requireManager(p, a);
        Volume v = a.volumes.stream().filter(b -> b.isCore(l.getBlockX(), l.getBlockY(), l.getBlockZ())).findFirst().orElseThrow();
        if (v == a.volumes.getFirst() && a.volumes.size() > 1) throw new IllegalArgumentException("请先拆除全部扩展核心");
        // Build drop before committing, so a broken configuration cannot erase a claim.
        var drop = cores.drop(v.core());
        change(() -> { if (a.volumes.size() == 1) index.remove(a); else { a.volumes.remove(v); index.reindex(a); } });
        l.getBlock().setType(Material.AIR); l.getWorld().dropItemNaturally(l.clone().add(.5, .5, .5), drop);
        message(p, "领地核心已拆除");
    }
    public void addMember(Player p, Area a, String name) {
        requireManager(p, a);
        if (!name.matches("[A-Za-z0-9_]{1,16}")) throw new IllegalArgumentException("请输入有效的玩家名称");
        var player = Bukkit.getPlayerExact(name);
        var known = player != null ? player : Bukkit.getOfflinePlayerIfCached(name);
        if (known == null || (!known.isOnline() && !known.hasPlayedBefore())) throw new IllegalArgumentException("玩家必须曾经加入此服务器");
        if (a.isOwner(known.getUniqueId(), name) || a.member(known.getUniqueId(), name) != null) throw new IllegalArgumentException("该玩家已是领地主人或成员");
        if (a.members.size() >= plugin.getConfig().getInt("Settings.MemberLimit", 100)) throw new IllegalArgumentException("成员达到上限");
        change(() -> a.members.put(known.getUniqueId().toString(), new Area.Member(Objects.requireNonNullElse(known.getName(), name))));
    }
    public String memberKey(Area a, String name) {
        return a.members.entrySet().stream().filter(e -> e.getValue().name.equalsIgnoreCase(name)).map(Map.Entry::getKey).findFirst()
                .orElseThrow(() -> new IllegalArgumentException("该玩家不是领地成员"));
    }
    public void bindLegacy(Player p) {
        if (!healthy) return;
        boolean needed = index.all().stream().anyMatch(a -> a.owner == null && a.ownerName.equalsIgnoreCase(p.getName()) || a.members.containsKey(p.getName().toLowerCase(Locale.ROOT)));
        if (needed) change(() -> index.all().forEach(a -> {
            if (a.owner == null && a.ownerName.equalsIgnoreCase(p.getName())) a.owner = p.getUniqueId();
            Area.Member m = a.members.remove(p.getName().toLowerCase(Locale.ROOT)); if (m != null) a.members.put(p.getUniqueId().toString(), m);
        }));
    }
    public void importLegacy() throws Exception {
        checkHealthy(); List<Area> legacy = store.readLegacy(plugin.getDataFolder().toPath().resolve("legacy-save"));
        AreaIndex candidate = new AreaIndex(); index.all().forEach(candidate::add);
        for (Area a : legacy) {
            if (candidate.get(a.id) != null) continue;
            for (Flag f : Flag.values()) a.flags.putIfAbsent(f, plugin.getConfig().getBoolean("Settings.Flags." + f.id, f.defaultValue));
            candidate.add(a);
        }
        store.save(candidate); index = candidate;
        for (Player p : Bukkit.getOnlinePlayers()) bindLegacy(p);
    }
    public void particles() {
        if (!healthy) return;
        int period = Math.max(1, plugin.getConfig().getInt("Settings.Particle.Period", 2));
        if (++particleTick % period != 0) return;
        double range = Math.max(1, Math.min(128, plugin.getConfig().getDouble("Settings.Particle.Range", 50)));
        double step = Math.max(.5, plugin.getConfig().getDouble("Settings.Particle.Distance", 1.5));
        int budget = Math.max(0, Math.min(2000, plugin.getConfig().getInt("Settings.Particle.Budget", 500)));
        for (Player p : Bukkit.getOnlinePlayers()) {
            int left = budget;
            for (Area a : index.all()) {
                if (left <= 0) break;
                if (!a.world.equals(p.getWorld().getName()) || !a.enabled(Flag.PARTICLE)) continue;
                for (Volume v : a.volumes) {
                    Location c = new Location(p.getWorld(), v.x(), v.y(), v.z());
                    if (p.getLocation().distanceSquared(c) > Math.pow(range + Math.max(v.rx(), v.rz()), 2)) continue;
                    left = draw(p, v, step, range, left);
                }
            }
        }
    }
    private int draw(Player p, Volume v, double step, double range, int budget) {
        double[] min = {v.minX(), v.minY(), v.minZ()}, max = {v.maxX()+1., v.maxY()+1., v.maxZ()+1.};
        for (int axis = 0; axis < 3; axis++) for (int a = 0; a < 2; a++) for (int b = 0; b < 2; b++) {
            for (double t = min[axis]; t <= max[axis] && budget > 0; t += step) {
                double[] pos = min.clone(); pos[axis] = t; pos[(axis+1)%3] = a == 0 ? min[(axis+1)%3] : max[(axis+1)%3]; pos[(axis+2)%3] = b == 0 ? min[(axis+2)%3] : max[(axis+2)%3];
                Location l = new Location(p.getWorld(), pos[0], pos[1], pos[2]);
                if (p.getLocation().distanceSquared(l) <= range * range) { p.spawnParticle(Particle.END_ROD, l, 1, 0, 0, 0, 0); budget--; }
            }
        }
        return budget;
    }
    public static void message(org.bukkit.command.CommandSender p, String text) { p.sendMessage("§e[PurtmarsArea] §f" + CoreItems.color(text)); }
}
