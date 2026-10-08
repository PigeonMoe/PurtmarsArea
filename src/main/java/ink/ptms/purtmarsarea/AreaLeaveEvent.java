package ink.ptms.purtmarsarea;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;
public final class AreaLeaveEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Area area;
    public AreaLeaveEvent(Player player, Area area) { super(player); this.area = area; }
    public Area getArea() { return area; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
