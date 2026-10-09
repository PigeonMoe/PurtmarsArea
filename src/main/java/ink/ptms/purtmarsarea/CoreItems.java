package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.configuration.ConfigurationSection;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.*;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.plugin.java.JavaPlugin;
import java.util.*;

public final class CoreItems {
    private final JavaPlugin plugin;
    private final NamespacedKey key, brokenKey;
    public CoreItems(JavaPlugin plugin) { this.plugin = plugin; key = new NamespacedKey(plugin, "core"); brokenKey = new NamespacedKey(plugin,"broken-core"); }
    public Set<String> ids() { return Objects.requireNonNull(plugin.getConfig().getConfigurationSection("Cores")).getKeys(false); }
    public String id(ItemStack item) {
        return item == null || !item.hasItemMeta() || isBroken(item) ? null : item.getItemMeta().getPersistentDataContainer().get(key, PersistentDataType.STRING);
    }
    public Volume volume(String id, Location location) {
        Volume v = configuredVolume(id,location.getBlockX(),location.getBlockY(),location.getBlockZ());
        World w = Objects.requireNonNull(location.getWorld());
        if (v.minY() < w.getMinHeight() || v.maxY() >= w.getMaxHeight()) throw new IllegalArgumentException("领地范围超出世界高度");
        return v;
    }
    public ItemStack create(String id) {
        ItemStack item = build(section(id));
        if (!item.getType().isBlock() || !item.getType().isSolid() || item.getType().isAir() || item.getType().hasGravity()) throw new IllegalArgumentException("核心必须是不受重力影响的方块");
        ItemMeta meta = item.getItemMeta(); meta.getPersistentDataContainer().remove(brokenKey); meta.getPersistentDataContainer().set(key, PersistentDataType.STRING, id); item.setItemMeta(meta); return item;
    }
    public boolean isBroken(ItemStack item) {
        return item != null && item.hasItemMeta() && item.getItemMeta().getPersistentDataContainer().has(brokenKey,PersistentDataType.STRING);
    }
    Volume configuredVolume(String id, int x, int y, int z) {
        ConfigurationSection s = section(id);
        boolean exact = s.isConfigurationSection("dimensions");
        if (exact && s.contains("size")) throw new IllegalArgumentException("核心不能同时配置 dimensions 和旧 size");
        String node = exact ? "dimensions" : "size";
        int sx = s.getInt(node+".x"), sy = s.getInt(node+".y"), sz = s.getInt(node+".z");
        Volume v = exact ? Volume.sized(x,y,z,sx,sy,sz,id) : new Volume(x,y,z,sx,sy,sz,id);
        if (exact) {
            int max = Math.min(256,plugin.getConfig().getInt("Settings.MaxSideLength",256));
            if (Math.max(sx,Math.max(sy,sz)) > max) throw new IllegalArgumentException("领地核心边长超过服务器限制");
        } else if (Math.max(v.rx(),Math.max(v.ry(),v.rz())) > Math.min(128,plugin.getConfig().getInt("Settings.MaxRadius",128)))
            throw new IllegalArgumentException("旧格式领地核心半径超过服务器限制");
        return v;
    }
    public ItemStack drop(String id) {
        ConfigurationSection core = section(id), drop = core.getConfigurationSection("drop");
        if (drop == null) {
            drop = new org.bukkit.configuration.MemoryConfiguration(); drop.set("material","GHAST_TEAR");
            drop.set("name","&7破碎的" + ChatColor.stripColor(color(core.getString("name","领地核心"))));
        }
        ItemStack item = build(drop); ItemMeta meta = item.getItemMeta();
        List<String> lore = new ArrayList<>(Objects.requireNonNullElse(meta.getLore(),List.of()));
        if (lore.stream().noneMatch(line -> ChatColor.stripColor(line).contains("修补后可继续使用"))) lore.add("§7修补后可继续使用");
        meta.setLore(lore); meta.getPersistentDataContainer().remove(key);
        meta.getPersistentDataContainer().set(brokenKey,PersistentDataType.STRING,id); item.setItemMeta(meta); return item;
    }
    public void validate() { for (String id : ids()) { configuredVolume(id,0,64,0); create(id); drop(id); } }
    private ConfigurationSection section(String id) {
        ConfigurationSection s = plugin.getConfig().getConfigurationSection("Cores." + id);
        if (s == null) throw new IllegalArgumentException("未知核心：" + id); return s;
    }
    private ItemStack build(ConfigurationSection s) {
        Material material = Material.matchMaterial(s.getString("material", "WHITE_STAINED_GLASS"));
        if (material == null || material.isAir() || !material.isItem()) throw new IllegalArgumentException("请使用 26.2 材质名称，不能使用旧数字 ID");
        ItemStack item = new ItemStack(material); ItemMeta meta = item.getItemMeta();
        meta.setDisplayName(color(s.getString("name", "领地核心"))); meta.setLore(s.getStringList("lore").stream().map(CoreItems::color).toList());
        if (s.getBoolean("shiny")) { meta.addEnchant(Enchantment.UNBREAKING, 1, true); meta.addItemFlags(ItemFlag.HIDE_ENCHANTS); }
        item.setItemMeta(meta); return item;
    }
    public static String color(String value) { return ChatColor.translateAlternateColorCodes('&', value); }
}
