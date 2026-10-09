package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.entity.Player;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.inventory.*;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.event.inventory.*;
import org.bukkit.scheduler.BukkitScheduler;
import org.junit.jupiter.api.*;
import org.junit.jupiter.api.io.TempDir;
import org.mockito.*;
import org.bukkit.plugin.java.JavaPlugin;
import java.nio.file.Path;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class MenuTest {
    @TempDir Path dir;
    AreaService s; Area a; Player p; MockedStatic<Bukkit> bukkit; MockedConstruction<ItemStack> items;
    Inventory current; List<Runnable> queued;
    List<Integer> sizes; List<String> titles; List<Map<Integer,ItemStack>> contents;
    @BeforeEach void setup() {
        JavaPlugin plugin = mock(JavaPlugin.class); YamlConfiguration cfg = new YamlConfiguration(); cfg.set("Settings.IgnoreFlags",List.of("teleport"));
        when(plugin.getConfig()).thenReturn(cfg); when(plugin.getDataFolder()).thenReturn(dir.toFile());
        p = mock(Player.class); when(p.getUniqueId()).thenReturn(UUID.randomUUID()); when(p.getName()).thenReturn("Owner");
        s = new AreaService(plugin,mock(CoreItems.class)); s.healthy = true;
        a = new Area(UUID.randomUUID(),"world",p.getUniqueId(),"Owner",Volume.sized(0,64,0,8,8,8,"0")); s.index.add(a);
        queued = new ArrayList<>(); when(p.isOnline()).thenReturn(true);
        sizes = new ArrayList<>(); titles = new ArrayList<>(); contents = new ArrayList<>();
        bukkit = mockStatic(Bukkit.class); bukkit.when(() -> Bukkit.createInventory(any(InventoryHolder.class),anyInt(),anyString())).thenAnswer(i -> {
            Inventory inv = mock(Inventory.class); current = inv; when(inv.getHolder()).thenReturn(i.getArgument(0)); int size = i.getArgument(1); sizes.add(size); titles.add(i.getArgument(2));
            Map<Integer,ItemStack> slots = new HashMap<>(); contents.add(slots); when(inv.getSize()).thenReturn(size);
            doAnswer(call -> { slots.put(call.getArgument(0),call.getArgument(1)); return null; }).when(inv).setItem(anyInt(),any(ItemStack.class));
            when(inv.getItem(anyInt())).thenAnswer(call -> slots.get(call.getArgument(0))); return inv;
        });
        BukkitScheduler scheduler = mock(BukkitScheduler.class); bukkit.when(Bukkit::getScheduler).thenReturn(scheduler);
        when(scheduler.runTask(eq(plugin),any(Runnable.class))).thenAnswer(i -> { queued.add(i.getArgument(1)); return null; });
        when(p.getOpenInventory()).thenAnswer(i -> { InventoryView view = mock(InventoryView.class); when(view.getTopInventory()).thenReturn(current); return view; });
        items = mockConstruction(ItemStack.class,(item,context) -> { when(item.getType()).thenReturn((Material)context.arguments().getFirst()); when(item.getItemMeta()).thenReturn(mock(ItemMeta.class)); });
    }
    @AfterEach void close() { items.close(); bukkit.close(); }
    @Test void homeHasNineSlotsAndExactBookText() {
        s.menus.home(p,a); assertEquals(List.of(9),sizes); assertEquals(List.of("领地核心"),titles);
        assertEquals(Set.of(4),contents.getFirst().keySet()); ItemStack book = contents.getFirst().get(4); assertEquals(Material.BOOK,book.getType());
        verify(book.getItemMeta()).setDisplayName("§f领地核心"); verify(book.getItemMeta()).setLore(List.of("§7左键: 领地设置","§7右键: 成员管理"));
    }
    @Test void flagGridUsesScreenshotSlotsAndPreservesAdditionalRulesOnSecondPage() {
        s.menus.flags(p,a,null); Map<Integer,ItemStack> slots = contents.getFirst(); assertEquals("领地核心 > 领地设置",titles.getFirst()); assertEquals(54,sizes.getFirst());
        assertEquals(Material.REDSTONE_BLOCK,slots.get(49).getType()); assertEquals(Material.ARROW,slots.get(53).getType());
        for (int slot : AreaMenus.FLAG_SLOTS) assertTrue(Set.of(Material.GREEN_TERRACOTTA,Material.RED_TERRACOTTA).contains(slots.get(slot).getType()));
        List<Flag> visible = s.menus.visibleFlags(false); assertEquals(25,visible.size()); assertFalse(visible.contains(Flag.TELEPORT));
        assertEquals(List.of(Flag.EXPLOSION,Flag.FIRE,Flag.PVP),visible.subList(22,25));
    }
    @Test void memberPagesRetainAllHeadsAndAddSignText() {
        for (int i = 0; i < 30; i++) a.members.put(UUID.randomUUID().toString(),new Area.Member("Member"+i));
        s.menus.members(p,a,0); assertEquals("领地核心 > 成员管理",titles.getFirst()); Map<Integer,ItemStack> first = contents.getFirst();
        for (int slot : AreaMenus.MEMBER_SLOTS) assertEquals(Material.PLAYER_HEAD,first.get(slot).getType());
        ItemStack sign = first.get(49); assertEquals(Material.OAK_SIGN,sign.getType()); verify(sign.getItemMeta()).setDisplayName("§f添加成员"); verify(sign.getItemMeta()).setLore(List.of("§7点击添加领地成员"));
        s.menus.members(p,a,1); Map<Integer,ItemStack> second = contents.getLast();
        assertEquals(2,second.values().stream().filter(item -> item.getType() == Material.PLAYER_HEAD).count()); assertNotNull(second.get(45)); assertNull(second.get(53));
    }
    private void click(int slot, ClickType type) {
        InventoryClickEvent event = mock(InventoryClickEvent.class); InventoryView view = mock(InventoryView.class);
        when(view.getTopInventory()).thenReturn(current); when(event.getView()).thenReturn(view); when(event.getWhoClicked()).thenReturn(p);
        when(event.getRawSlot()).thenReturn(slot); when(event.getClick()).thenReturn(type);
        s.menus.click(event); verify(event).setCancelled(true); queued.removeFirst().run();
    }
    @Test void bookRoutesLeftToSettingsAndRightToMembersAndExtraFlagsRenderOnNextPage() {
        s.menus.home(p,a); click(4,ClickType.LEFT); assertEquals(AreaMenus.FLAGS_TITLE,titles.getLast());
        click(53,ClickType.LEFT); Map<Integer,ItemStack> second = contents.getLast();
        assertEquals(Set.of(10,11,12,45,49),second.keySet());
        verify(second.get(10).getItemMeta()).setDisplayName("§f爆炸破坏"); verify(second.get(11).getItemMeta()).setDisplayName("§f火焰蔓延与燃烧");
        verify(second.get(12).getItemMeta()).setDisplayName("§f玩家战斗");
        click(49,ClickType.LEFT); assertEquals(AreaMenus.HOME_TITLE,titles.getLast()); click(4,ClickType.RIGHT);
        assertEquals(AreaMenus.MEMBERS_TITLE,titles.getLast());
    }
}
