package ink.ptms.purtmarsarea;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;
/** Notification after a permitted move/teleport crosses into a claim. */
public final class AreaEnterEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Area area;
    public AreaEnterEvent(Player player, Area area) { super(player); this.area = area; }
    public Area getArea() { return area; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
