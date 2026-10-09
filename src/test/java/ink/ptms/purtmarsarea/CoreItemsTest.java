package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.inventory.*;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.persistence.*;
import org.bukkit.plugin.java.JavaPlugin;
import org.junit.jupiter.api.*;
import org.mockito.MockedConstruction;
import org.mockito.MockedStatic;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class CoreItemsTest {
    JavaPlugin plugin; YamlConfiguration config; CoreItems cores;
    @BeforeEach void setup() {
        plugin = mock(JavaPlugin.class); when(plugin.getName()).thenReturn("PurtmarsArea"); when(plugin.namespace()).thenReturn("purtmarsarea");
        config = YamlConfiguration.loadConfiguration(new java.io.File("src/main/resources/config.yml"));
        when(plugin.getConfig()).thenReturn(config); cores = new CoreItems(plugin);
    }
    @Test void fourDefaultsAreActualLengthsAndLegacySizeKeepsRadiusMeaning() {
        assertEquals(Set.of("0","1","2","3"),cores.ids()); int n = 8;
        for (String id : List.of("0","1","2","3")) { Volume v = cores.configuredVolume(id,-17,64,0); assertEquals(n,v.sizeX()); assertEquals(n,v.sizeY()); assertEquals(n,v.sizeZ()); n *= 2; }
        config.set("Cores.0.dimensions",null); config.set("Cores.0.size.x",8); config.set("Cores.0.size.y",8); config.set("Cores.0.size.z",8);
        assertEquals(new Volume(-17,64,0,8,8,8,"0"),cores.configuredVolume("0",-17,64,0));
    }
    @Test void ambiguousConfigurationAndWorldHeightOverflowAreRejected() {
        config.set("Cores.0.size.x",8); assertThrows(IllegalArgumentException.class,() -> cores.configuredVolume("0",0,64,0)); config.set("Cores.0.size",null);
        World world = mock(World.class); when(world.getMinHeight()).thenReturn(-64); when(world.getMaxHeight()).thenReturn(320);
        assertThrows(IllegalArgumentException.class,() -> cores.volume("0",new Location(world,0,-61,0)));
        assertDoesNotThrow(() -> cores.volume("0",new Location(world,0,-60,0)));
        assertThrows(IllegalArgumentException.class,() -> cores.volume("3",new Location(world,0,289,0)));
    }
    @Test void brokenDropsHaveDistinctMarksNamesAndRepairLoreAndNeverIdentifyAsComplete() {
        config.getConfigurationSection("Cores").getKeys(false).forEach(id -> config.set("Cores."+id+".shiny",false));
        Material glass = mock(Material.class), tear = mock(Material.class);
        when(glass.isBlock()).thenReturn(true); when(glass.isSolid()).thenReturn(true); when(glass.isItem()).thenReturn(true); when(tear.isItem()).thenReturn(true);
        try (MockedStatic<Material> materials = mockStatic(Material.class);
             MockedConstruction<ItemStack> stacks = mockConstruction(ItemStack.class,(item,context) -> {
            ItemMeta meta = mock(ItemMeta.class); PersistentDataContainer pdc = mock(PersistentDataContainer.class); Map<NamespacedKey,String> data = new HashMap<>();
            when(item.getItemMeta()).thenReturn(meta); when(item.hasItemMeta()).thenReturn(true); when(item.getType()).thenReturn((Material)context.arguments().getFirst());
            when(meta.getPersistentDataContainer()).thenReturn(pdc);
            when(pdc.get(any(NamespacedKey.class),eq(PersistentDataType.STRING))).thenAnswer(i -> data.get(i.getArgument(0)));
            when(pdc.has(any(NamespacedKey.class),eq(PersistentDataType.STRING))).thenAnswer(i -> data.containsKey(i.getArgument(0)));
            doAnswer(i -> { data.put(i.getArgument(0),i.getArgument(2)); return null; }).when(pdc).set(any(NamespacedKey.class),eq(PersistentDataType.STRING),anyString());
            doAnswer(i -> { data.remove(i.getArgument(0)); return null; }).when(pdc).remove(any(NamespacedKey.class));
        })) {
            materials.when(() -> Material.matchMaterial("WHITE_STAINED_GLASS")).thenReturn(glass);
            materials.when(() -> Material.matchMaterial("GHAST_TEAR")).thenReturn(tear);
            String[] names = {"小型","中型","大型","超大型"};
            for (int i = 0; i < names.length; i++) {
                ItemStack drop = cores.drop(""+i); assertTrue(cores.isBroken(drop)); assertNull(cores.id(drop));
                verify(drop.getItemMeta()).setDisplayName("§7破碎的"+names[i]+"领地核心");
                verify(drop.getItemMeta(),atLeastOnce()).setLore(List.of("§7修补后可继续使用"));
                ItemStack complete = cores.create(""+i); assertFalse(cores.isBroken(complete)); assertEquals(""+i,cores.id(complete));
            }
        }
    }
}
