# Debugging Examples

Store examples of successfully debugged issues, detailing the symptoms, root cause, and fix.

## Example: Null Pointer Exception on User profile

### Symptoms
- 500 error when clicking Profile page.
- Console error: `Cannot read properties of undefined (reading 'name')`.

### Root Cause
- Profile data loader was returning `null` for users without complete profiles, and the UI was trying to read `user.name` without a safety check.

### Fix
- Used optional chaining (`user?.name`) and loaded fallback default details.
