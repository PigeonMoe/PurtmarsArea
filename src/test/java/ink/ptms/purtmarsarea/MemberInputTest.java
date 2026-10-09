package ink.ptms.purtmarsarea;

import io.papermc.paper.event.player.AsyncChatEvent;
import net.kyori.adventure.text.Component;
import org.bukkit.*;
import org.bukkit.entity.Player;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.event.player.*;
import org.bukkit.event.inventory.InventoryOpenEvent;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.scheduler.*;
import org.junit.jupiter.api.*;
import org.junit.jupiter.api.io.TempDir;
import org.mockito.MockedStatic;
import java.nio.file.Path;
import java.util.*;
import java.util.logging.Logger;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class MemberInputTest {
    @TempDir Path dir;
    MockedStatic<Bukkit> bukkit;
    AreaService s; Player owner, member; Area area; World world; BukkitScheduler scheduler; BukkitTask timer;
    List<Runnable> queued, timeouts;
    @BeforeEach void setup() {
        JavaPlugin plugin = mock(JavaPlugin.class); YamlConfiguration cfg = new YamlConfiguration(); cfg.set("Settings.MemberLimit",100);
        when(plugin.getConfig()).thenReturn(cfg); when(plugin.getDataFolder()).thenReturn(dir.toFile()); when(plugin.getLogger()).thenReturn(Logger.getLogger("input-test"));
        owner = mock(Player.class); when(owner.getUniqueId()).thenReturn(UUID.randomUUID()); when(owner.getName()).thenReturn("Owner"); when(owner.isOnline()).thenReturn(true);
        member = mock(Player.class); when(member.getUniqueId()).thenReturn(UUID.randomUUID()); when(member.getName()).thenReturn("Member"); when(member.isOnline()).thenReturn(true);
        world = mock(World.class); when(world.getUID()).thenReturn(UUID.randomUUID()); when(world.getName()).thenReturn("world");
        when(owner.getWorld()).thenReturn(world); when(owner.getLocation()).thenReturn(new Location(world,0,64,0));
        s = new AreaService(plugin,mock(CoreItems.class)); s.healthy = true;
        area = new Area(UUID.randomUUID(),"world",owner.getUniqueId(),"Owner",Volume.sized(0,64,0,8,8,8,"0")); s.index.add(area);
        scheduler = mock(BukkitScheduler.class); timer = mock(BukkitTask.class); queued = new ArrayList<>(); timeouts = new ArrayList<>();
        when(scheduler.runTask(eq(plugin),any(Runnable.class))).thenAnswer(i -> { queued.add(i.getArgument(1)); return mock(BukkitTask.class); });
        when(scheduler.runTaskLater(eq(plugin),any(Runnable.class),anyLong())).thenAnswer(i -> { timeouts.add(i.getArgument(1)); return timer; });
        bukkit = mockStatic(Bukkit.class); bukkit.when(Bukkit::getScheduler).thenReturn(scheduler); bukkit.when(() -> Bukkit.getPlayerExact("Member")).thenReturn(member);
    }
    @AfterEach void close() { bukkit.close(); }
    AsyncChatEvent input(String value) {
        AsyncChatEvent e = mock(AsyncChatEvent.class); when(e.getPlayer()).thenReturn(owner); when(e.message()).thenReturn(Component.text(value)); when(e.viewers()).thenReturn(new HashSet<>());
        s.memberInput.chat(e); return e;
    }
    @Test void privateInputQueuesAllGameChecksUntilMainThread() {
        s.memberInput.begin(owner,area); clearInvocations(owner,member,world);
        AsyncChatEvent e = input("Member"); verify(e).setCancelled(true); verifyNoInteractions(owner,member,world); assertTrue(area.members.isEmpty());
        queued.getFirst().run(); assertNotNull(area.member(member.getUniqueId(),"Member")); verify(timer).cancel();
    }
    @Test void duplicateChatBeforeMainCallbackIsConsumedOnlyOnce() {
        s.memberInput.begin(owner,area); input("Member"); AsyncChatEvent second = input("Member");
        verify(second).setCancelled(true); assertEquals(1,queued.size()); queued.getFirst().run(); assertEquals(1,area.members.size());
    }
    @Test void cancelAndTimeoutNeverAddMember() {
        s.memberInput.begin(owner,area); input("取消"); queued.getFirst().run(); assertTrue(area.members.isEmpty());
        s.memberInput.begin(owner,area); timeouts.getLast().run(); AsyncChatEvent e = input("Member"); verify(e,never()).setCancelled(true);
    }
    @Test void replacedSessionDiscardsAlreadyQueuedReply() {
        s.memberInput.begin(owner,area); input("Member"); s.memberInput.begin(owner,area); queued.getFirst().run(); assertTrue(area.members.isEmpty());
        input("Member"); queued.getLast().run(); assertEquals(1,area.members.size());
    }
    @Test void reloadDiscardsReplyAndQuitClearsSession() {
        s.memberInput.begin(owner,area); input("Member"); s.reload(); queued.getFirst().run(); assertTrue(area.members.isEmpty());
        s.index.add(area); s.memberInput.begin(owner,area); PlayerQuitEvent quit = mock(PlayerQuitEvent.class); when(quit.getPlayer()).thenReturn(owner); s.memberInput.quit(quit);
        AsyncChatEvent e = input("Member"); verify(e,never()).setCancelled(true);
    }
    @Test void deletedOrReplacedAreaCannotBeEdited() {
        s.memberInput.begin(owner,area); input("Member"); s.index.remove(area);
        Area replacement = new Area(area.id,"world",owner.getUniqueId(),"Owner",area.volumes.getFirst()); s.index.add(replacement);
        queued.getFirst().run(); assertTrue(area.members.isEmpty()); assertTrue(replacement.members.isEmpty());
    }
    @Test void ownershipIsRecheckedOnReply() {
        s.memberInput.begin(owner,area); input("Member"); area.owner = UUID.randomUUID(); queued.getFirst().run(); assertTrue(area.members.isEmpty());
    }
    @Test void limitsAndDuplicatesAreRecheckedOnReply() {
        s.memberInput.begin(owner,area); input("Member"); s.plugin.getConfig().set("Settings.MemberLimit",0); queued.getFirst().run(); assertTrue(area.members.isEmpty());
        s.plugin.getConfig().set("Settings.MemberLimit",100); s.memberInput.begin(owner,area); input("Member");
        Area.Member existing = new Area.Member("Member"); area.members.put(member.getUniqueId().toString(),existing); queued.getLast().run();
        assertEquals(1,area.members.size()); assertSame(existing,area.member(member.getUniqueId(),"Member"));
    }
    @Test void changedWorldOrPhysicalClaimRejectsReply() {
        s.memberInput.begin(owner,area); input("Member"); World other = mock(World.class); when(other.getUID()).thenReturn(UUID.randomUUID()); when(owner.getWorld()).thenReturn(other);
        queued.getFirst().run(); assertTrue(area.members.isEmpty());
        when(owner.getWorld()).thenReturn(world); s.memberInput.begin(owner,area); input("Member"); when(owner.getLocation()).thenReturn(new Location(world,100,64,0));
        queued.getLast().run(); assertTrue(area.members.isEmpty());
    }
    @Test void invalidPlayerNameNeverLooksUpOrAddsPlayer() {
        s.memberInput.begin(owner,area); input("/pa add Member"); queued.getFirst().run(); assertTrue(area.members.isEmpty());
        bukkit.verify(() -> Bukkit.getPlayerExact(anyString()),never());
    }
    @Test void timeoutAfterChatBeforeMainTaskDiscardsQueuedReply() {
        s.memberInput.begin(owner,area); input("Member"); timeouts.getFirst().run(); queued.getFirst().run(); assertTrue(area.members.isEmpty());
    }
    @Test void openingAnotherInventoryAndChangingWorldCancelPendingInput() {
        s.memberInput.begin(owner,area); InventoryOpenEvent event = mock(InventoryOpenEvent.class); when(event.getPlayer()).thenReturn(owner);
        s.memberInput.open(event); verify(input("Member"),never()).setCancelled(true);
        s.memberInput.begin(owner,area); PlayerChangedWorldEvent changed = mock(PlayerChangedWorldEvent.class); when(changed.getPlayer()).thenReturn(owner);
        s.memberInput.world(changed); verify(input("Member"),never()).setCancelled(true);
    }
    @Test void pluginCloseInvalidatesQueuedInputAndUnknownPlayersAreRejected() {
        s.memberInput.begin(owner,area); input("Member"); s.close(); queued.getFirst().run(); assertTrue(area.members.isEmpty());
        s.memberInput.begin(owner,area); input("Unknown"); queued.getLast().run(); assertTrue(area.members.isEmpty());
    }
}
