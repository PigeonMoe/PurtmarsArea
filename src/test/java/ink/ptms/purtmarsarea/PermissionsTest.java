package ink.ptms.purtmarsarea;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.EnumSource;
import java.util.UUID;
import static org.junit.jupiter.api.Assertions.*;

class PermissionsTest {
    UUID owner = UUID.randomUUID(), visitor = UUID.randomUUID();
    Area a = new Area(UUID.randomUUID(), "world", owner, "owner",new Volume(0,64,0,8,8,8,"0"));
    @ParameterizedTest @EnumSource(Flag.class)
    void ownerAlwaysHasPersonalAccessAndVisitorUsesDefaults(Flag flag) {
        assertTrue(a.allows(flag,owner,"renamed-owner"));
        assertEquals(flag.defaultValue,a.allows(flag,visitor,"visitor"));
    }
    @Test void membershipDoesNotGrantBuildAndDoesNotInheritPublicChanges() {
        Area.Member m = new Area.Member("member"); a.members.put(visitor.toString(),m); a.flags.put(Flag.BUILD,true);
        assertFalse(a.allows(Flag.BUILD,visitor,"member")); m.flags.put(Flag.BUILD,true); assertTrue(a.allows(Flag.BUILD,visitor,"member"));
        m.flags.put(Flag.BUILD,false); assertFalse(a.allows(Flag.BUILD,visitor,"renamed-member"));
    }
    @Test void uuidIdentityPreventsOldOwnerNameFromGrantingAccess() {
        a.flags.put(Flag.BUILD,false); assertFalse(a.allows(Flag.BUILD,visitor,"owner"));
    }
    @Test void legacyNamesAreMatchedCaseInsensitivelyUntilBound() {
        a.owner = null; assertTrue(a.isOwner(visitor,"OwNeR"));
        Area.Member m = new Area.Member("Legacy"); m.flags.put(Flag.CONTAINER,true); a.members.put("legacy",m);
        assertTrue(a.allows(Flag.CONTAINER,visitor,"LEGACY"));
    }
    @Test void environmentalPermissionsCannotBeOverriddenByMemberMap() {
        Area.Member m = new Area.Member("member"); m.flags.put(Flag.PVP,true); a.members.put(visitor.toString(),m); a.flags.put(Flag.PVP,false);
        assertFalse(a.allows(Flag.PVP,visitor,"member"));
    }
    @Test void memberDefaultsFollowServerConfigurationIndependentlyOfVisitorSetting() {
        a.flags.put(Flag.BUILD,false); a.members.put(visitor.toString(),new Area.Member("member"));
        assertTrue(a.allows(Flag.BUILD,visitor,"member",true));
        assertFalse(a.allows(Flag.BUILD,UUID.randomUUID(),"stranger",true));
    }
}
