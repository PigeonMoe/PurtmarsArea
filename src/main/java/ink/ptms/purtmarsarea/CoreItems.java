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
    private final NamespacedKey key;
    public CoreItems(JavaPlugin plugin) { this.plugin = plugin; key = new NamespacedKey(plugin, "core"); }
    public Set<String> ids() { return Objects.requireNonNull(plugin.getConfig().getConfigurationSection("Cores")).getKeys(false); }
    public String id(ItemStack item) {
        return item == null || !item.hasItemMeta() ? null : item.getItemMeta().getPersistentDataContainer().get(key, PersistentDataType.STRING);
    }
    public Volume volume(String id, Location location) {
        ConfigurationSection s = section(id);
        Volume v = new Volume(location.getBlockX(), location.getBlockY(), location.getBlockZ(),
                s.getInt("size.x"), s.getInt("size.y"), s.getInt("size.z"), id);
        int max = Math.min(128, plugin.getConfig().getInt("Settings.MaxRadius", 128));
        if (Math.max(v.rx(), Math.max(v.ry(), v.rz())) > max) throw new IllegalArgumentException("领地核心半径超过服务器限制");
        World w = Objects.requireNonNull(location.getWorld());
        if (v.minY() < w.getMinHeight() || v.maxY() >= w.getMaxHeight()) throw new IllegalArgumentException("领地范围超出世界高度");
        return v;
    }
    public ItemStack create(String id) {
        ItemStack item = build(section(id));
        if (!item.getType().isBlock() || !item.getType().isSolid() || item.getType().isAir() || item.getType().hasGravity()) throw new IllegalArgumentException("核心必须是不受重力影响的方块");
        ItemMeta meta = item.getItemMeta(); meta.getPersistentDataContainer().set(key, PersistentDataType.STRING, id); item.setItemMeta(meta); return item;
    }
    public ItemStack drop(String id) {
        ConfigurationSection s = section(id).getConfigurationSection("drop"); return s == null ? create(id) : build(s);
    }
    public void validate() { for (String id : ids()) { create(id); ConfigurationSection s = section(id); new Volume(0, 0, 0, s.getInt("size.x"), s.getInt("size.y"), s.getInt("size.z"), id); drop(id); } }
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
