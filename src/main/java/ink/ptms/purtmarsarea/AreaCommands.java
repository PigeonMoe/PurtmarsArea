package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.command.*;
import org.bukkit.entity.Player;
import org.bukkit.inventory.ItemStack;
import java.util.*;

public final class AreaCommands implements CommandExecutor, TabCompleter {
    private final AreaService s;
    private static final List<String> BASIC = List.of("help", "info", "menu", "list", "set", "add", "remove", "name", "welcome", "farewell");
    private static final List<String> ADMIN = List.of("item", "save", "load", "reload", "import", "mirror", "status");
    public AreaCommands(AreaService s) { this.s = s; }
    @Override public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        try {
            String sub = args.length == 0 ? "help" : args[0].toLowerCase(Locale.ROOT);
            if (ADMIN.contains(sub) && !s.admin(sender)) throw new IllegalArgumentException("需要管理员权限");
            switch (sub) {
                case "help" -> {
                    sender.sendMessage("§e领地管理 §f/pa info | menu | list | add 玩家 | remove 玩家");
                    sender.sendMessage("§f/pa set 权限 true/false [成员] | name 名称 | welcome 内容 | farewell 内容");
                    if (s.admin(sender)) sender.sendMessage("§7管理：/pa item 核心编号 [在线玩家] [数量] | save | reload | import | mirror | status");
                }
                case "status" -> AreaService.message(sender, (s.healthy ? "正常" : "保护锁定") + " | 领地 " + s.index.all().size() + " | TabooLib 6.3.0-0e3a911 | Paper 26.2");
                case "reload", "load" -> { s.reload(); AreaService.message(sender, s.healthy ? "配置与领地数据已载入" : "载入失败，系统已保护锁定，请查看日志"); }
                case "save" -> { s.save(); AreaService.message(sender, "领地数据已保存"); }
                case "import" -> { s.importLegacy(); AreaService.message(sender, "旧数据已导入，原文件保留在 legacy-save"); }
                case "mirror" -> { AreaService.message(sender, "保护拦截计数（本次启动）："); s.denials.forEach((f, n) -> sender.sendMessage("§7" + f.title + ": " + n)); }
                case "item" -> {
                    s.checkHealthy(); if (args.length < 2 || args.length > 4) throw new IllegalArgumentException("/pa item 核心编号 [在线玩家] [数量]");
                    Player target = args.length > 2 ? Bukkit.getPlayerExact(args[2]) : player(sender);
                    if (target == null) throw new IllegalArgumentException("玩家不在线");
                    int amount = args.length > 3 ? Integer.parseInt(args[3]) : 1;
                    if (amount < 1 || amount > 64) throw new IllegalArgumentException("数量必须为 1..64");
                    ItemStack item = s.cores.create(args[1]); item.setAmount(amount);
                    target.getInventory().addItem(item).values().forEach(i -> target.getWorld().dropItemNaturally(target.getLocation(), i));
                    AreaService.message(sender, "已给予 " + target.getName() + " 核心 ×" + amount);
                }
                case "list" -> { s.checkHealthy(); s.menus.list(player(sender), 0); }
                case "info" -> {
                    s.checkHealthy(); Area a = current(player(sender)); Volume v = a.volumes.getFirst();
                    AreaService.message(sender, a.name + " | 主人 " + a.ownerName + " | ID " + a.id);
                    sender.sendMessage("§7核心 " + a.world + " " + v.x()+","+v.y()+","+v.z() + " | 半径 " + v.rx()+","+v.ry()+","+v.rz());
                    sender.sendMessage("§7成员：" + String.join(", ", a.members.values().stream().map(m -> m.name).toList()) + " | 扩展 " + (a.volumes.size()-1));
                }
                case "menu" -> { Player p = player(sender); s.menus.home(p, current(p)); }
                case "set" -> {
                    if (args.length < 3 || args.length > 4) throw new IllegalArgumentException("/pa set 权限 true/false [成员]");
                    Player p = player(sender); Area a = current(p); s.requireManager(p, a);
                    Flag f = Flag.parse(args[1]); boolean member = args.length == 4; s.mutableFlag(f, member);
                    if (!args[2].equalsIgnoreCase("true") && !args[2].equalsIgnoreCase("false")) throw new IllegalArgumentException("权限值只能为 true 或 false");
                    boolean value = Boolean.parseBoolean(args[2]);
                    Map<Flag, Boolean> flags = member ? a.members.get(s.memberKey(a, args[3])).flags : a.flags;
                    s.change(() -> flags.put(f, value)); AreaService.message(p, "已设置 " + f.title + "：" + value);
                }
                case "add", "remove" -> {
                    if (args.length != 2) throw new IllegalArgumentException("/pa " + sub + " 玩家名");
                    Player p = player(sender); Area a = current(p); s.requireManager(p, a);
                    if (sub.equals("add")) s.addMember(p, a, args[1]);
                    else { String key = s.memberKey(a, args[1]); s.change(() -> a.members.remove(key)); }
                    AreaService.message(p, "成员已更新");
                }
                case "name", "welcome", "farewell" -> {
                    Player p = player(sender); Area a = current(p); s.requireManager(p, a);
                    if (args.length < 2) throw new IllegalArgumentException("/pa " + sub + " 内容");
                    String text = String.join(" ", Arrays.copyOfRange(args, 1, args.length));
                    if (text.length() > (sub.equals("name") ? 32 : 120)) throw new IllegalArgumentException("内容过长");
                    String value = !sub.equals("name") && text.equalsIgnoreCase("off") ? "" : text;
                    s.change(() -> { switch (sub) { case "name" -> a.name = value; case "welcome" -> a.welcome = value; default -> a.farewell = value; } });
                    AreaService.message(p, "领地显示已更新");
                }
                default -> throw new IllegalArgumentException("未知命令，使用 /pa help");
            }
        } catch (Exception ex) { AreaService.message(sender, Objects.requireNonNullElse(ex.getMessage(), "操作失败，请查看日志")); }
        return true;
    }
    private Player player(CommandSender sender) {
        if (!(sender instanceof Player p)) throw new IllegalArgumentException("该命令只能由玩家执行"); return p;
    }
    private Area current(Player p) {
        s.checkHealthy(); Area a = s.at(p.getLocation()); if (a == null) throw new IllegalArgumentException("当前位置没有领地"); return a;
    }
    @Override public List<String> onTabComplete(CommandSender sender, Command c, String alias, String[] args) {
        List<String> choices = new ArrayList<>();
        if (args.length == 1) { choices.addAll(BASIC); if (s.admin(sender)) choices.addAll(ADMIN); }
        else if (args.length > 1) {
            String sub = args[0].toLowerCase(Locale.ROOT);
            if (ADMIN.contains(sub) && !s.admin(sender)) return List.of();
            if (sub.equals("item") && args.length == 2) choices.addAll(s.cores.ids());
            if (sub.equals("set") && args.length == 2) Arrays.stream(Flag.values()).filter(f -> !s.plugin.getConfig().getStringList("Settings.IgnoreFlags").contains(f.id)).map(f -> f.id).forEach(choices::add);
            if (sub.equals("set") && args.length == 3) choices.addAll(List.of("true", "false"));
            if (sub.equals("add") && args.length == 2 || sub.equals("item") && args.length == 3) Bukkit.getOnlinePlayers().stream().map(Player::getName).forEach(choices::add);
            if (sender instanceof Player p && (sub.equals("remove") && args.length == 2 || sub.equals("set") && args.length == 4)) {
                Area a = s.at(p.getLocation()); if (a != null && s.manages(p, a)) a.members.values().stream().map(m -> m.name).forEach(choices::add);
            }
        }
        String prefix = args.length == 0 ? "" : args[args.length-1].toLowerCase(Locale.ROOT);
        return choices.stream().filter(x -> x.toLowerCase(Locale.ROOT).startsWith(prefix)).toList();
    }
}
