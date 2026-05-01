use std::collections::HashMap;

use winit::keyboard::KeyCode;

#[derive(Copy, Clone)]
pub enum InputAction {
    MoveLeft,
    MoveRight,
    MoveForward,
    MoveBackward,
    Afterburner,
    Bullet,
    Bomb,
    Mine,
    Thor,
    Repel,
    Burst,
    Rocket,
    Brick,
    Multifire,
    Antiwarp,
    Stealth,
    Cloak,
    XRadar,
    Warp,
    Portal,
    Decoy,
    Attach,
    StatboxCycle,
    StatboxUp,
    StatboxDown,
    FullRadar,
}

#[derive(Copy, Clone)]
pub enum InputModifier {
    Control,
    Shift,
    Alt,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct InputModifierSet {
    states: u8,
}

impl InputModifierSet {
    pub fn empty() -> Self {
        Self { states: 0 }
    }

    pub fn with(modifier: InputModifier) -> Self {
        let mut result = Self { states: 0 };

        result.set(modifier);

        result
    }

    pub fn set(&mut self, modifier: InputModifier) {
        self.states |= 1 << modifier as u8;
    }

    pub fn erase(&mut self, modifier: InputModifier) {
        self.states &= !(1 << modifier as u8);
    }

    pub fn is_set(&self, modifier: InputModifier) -> bool {
        self.states & (1 << modifier as u8) != 0
    }

    pub fn union(&self, other: Self) -> Self {
        let mut overlap = Self::empty();

        for i in 0..8 {
            if self.states & (1 << i) != 0 && other.states & (1 << i) != 0 {
                overlap.states |= 1 << i;
            }
        }

        overlap
    }

    pub fn count(&self) -> usize {
        let mut count = 0;

        for i in 0..8 {
            if self.states & (1 << i) != 0 {
                count += 1;
            }
        }

        count
    }
}

pub struct InputState {
    down_states: u64,
    trigger_states: u64,
    modifier_down_states: InputModifierSet,
    modifier_trigger_states: InputModifierSet,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            down_states: 0,
            trigger_states: 0,
            modifier_down_states: InputModifierSet::empty(),
            modifier_trigger_states: InputModifierSet::empty(),
        }
    }

    // An action is triggered only on the tick where it is immediately pressed down.
    // Holding it down long enough for OS repeating codes will trigger it again.
    pub fn is_triggered(&self, action: InputAction) -> bool {
        let index = action as u64;

        self.trigger_states & (1 << index) != 0
    }

    // An action is down any time that the key is pressed down. This includes repeating
    pub fn is_down(&self, action: InputAction) -> bool {
        let index = action as u64;

        self.down_states & (1 << index) != 0
    }

    pub fn is_modifier_down(&self, modifier: InputModifier) -> bool {
        self.modifier_down_states.is_set(modifier)
    }

    pub fn is_modifier_triggered(&self, modifier: InputModifier) -> bool {
        self.modifier_trigger_states.is_set(modifier)
    }

    pub fn clear_triggered(&mut self) {
        self.trigger_states = 0;
        self.modifier_trigger_states.states = 0;
    }

    pub fn set_triggered(&mut self, action: InputAction) {
        let index = action as u64;

        self.trigger_states |= 1 << index;
    }

    pub fn set_down(&mut self, action: InputAction, pressed: bool) {
        let index = action as u64;

        if pressed {
            self.down_states |= 1 << index;
        } else {
            self.down_states &= !(1 << index);
        }
    }

    pub fn set_modifier_down(&mut self, modifier: InputModifier, pressed: bool) {
        if pressed {
            self.modifier_down_states.set(modifier);
        } else {
            self.modifier_down_states.erase(modifier);
        }
    }

    pub fn set_modifier_triggered(&mut self, modifier: InputModifier) {
        self.modifier_trigger_states.set(modifier);
    }

    pub fn get_modifier_down_set(&self) -> InputModifierSet {
        self.modifier_down_states
    }
}

// This stores a list of actions associated with a keycode.
// It is used for determining which action should be triggered depending on the modifiers pressed.
struct KeyActionSet {
    actions: Vec<(InputAction, InputModifierSet)>,
}

impl KeyActionSet {
    pub fn new() -> Self {
        Self { actions: vec![] }
    }

    pub fn insert(&mut self, action: InputAction, modifiers: InputModifierSet) {
        self.actions.push((action, modifiers));
    }
}

pub struct InputMapping {
    mapping: HashMap<KeyCode, KeyActionSet>,
}

impl InputMapping {
    pub fn new() -> Self {
        Self {
            mapping: HashMap::new(),
        }
    }

    pub fn register_defaults(&mut self) {
        self.register_action(KeyCode::ArrowLeft, InputAction::MoveLeft);
        self.register_action(KeyCode::ArrowRight, InputAction::MoveRight);
        self.register_action(KeyCode::ArrowUp, InputAction::MoveForward);
        self.register_action(KeyCode::ArrowDown, InputAction::MoveBackward);

        self.register_action(KeyCode::AltLeft, InputAction::FullRadar);
        self.register_action(KeyCode::AltRight, InputAction::FullRadar);

        self.register_action(KeyCode::PageDown, InputAction::StatboxDown);
        self.register_action(KeyCode::PageUp, InputAction::StatboxUp);
        self.register_action(KeyCode::F2, InputAction::StatboxCycle);

        self.register_action(KeyCode::ShiftLeft, InputAction::Afterburner);
        self.register_action(KeyCode::ShiftRight, InputAction::Afterburner);

        self.register_action(KeyCode::ControlLeft, InputAction::Bullet);
        self.register_action(KeyCode::ControlRight, InputAction::Bullet);

        self.register_action(KeyCode::Tab, InputAction::Bomb);

        self.register_modifier_action(
            KeyCode::Tab,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Mine,
        );

        self.register_action(KeyCode::F6, InputAction::Thor);

        self.register_modifier_action(
            KeyCode::ShiftLeft,
            InputModifierSet::with(InputModifier::Control),
            InputAction::Repel,
        );
        self.register_modifier_action(
            KeyCode::ShiftRight,
            InputModifierSet::with(InputModifier::Control),
            InputAction::Repel,
        );
        self.register_modifier_action(
            KeyCode::ControlLeft,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Repel,
        );
        self.register_modifier_action(
            KeyCode::ControlRight,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Repel,
        );
        self.register_action(KeyCode::Backquote, InputAction::Repel);

        self.register_modifier_action(
            KeyCode::Delete,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Burst,
        );

        self.register_action(KeyCode::F3, InputAction::Rocket);
        self.register_action(KeyCode::F4, InputAction::Brick);

        self.register_action(KeyCode::Delete, InputAction::Multifire);

        self.register_modifier_action(
            KeyCode::End,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Antiwarp,
        );

        self.register_action(KeyCode::Home, InputAction::Stealth);

        self.register_modifier_action(
            KeyCode::Home,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Cloak,
        );

        self.register_action(KeyCode::End, InputAction::XRadar);

        self.register_action(KeyCode::Insert, InputAction::Warp);
        self.register_modifier_action(
            KeyCode::Insert,
            InputModifierSet::with(InputModifier::Shift),
            InputAction::Portal,
        );

        self.register_action(KeyCode::F5, InputAction::Decoy);
        self.register_action(KeyCode::F7, InputAction::Attach);
    }

    pub fn get_action(&self, code: KeyCode, input_state: &InputState) -> Option<InputAction> {
        let Some(key_set) = self.mapping.get(&code) else {
            return None;
        };

        let current_modifiers = input_state.get_modifier_down_set();

        let mut best_action = None;
        let mut best_action_modifier_count: usize = 0;

        // Prioritize actions that have the best overlap with modifiers.
        for (action, modifier_set) in &key_set.actions {
            let modifier_count = current_modifiers.union(*modifier_set).count();

            if best_action.is_none() || modifier_count > best_action_modifier_count {
                best_action = Some(action);
                best_action_modifier_count = modifier_count;
            }

            if *modifier_set == current_modifiers {
                return Some(*action);
            }
        }

        best_action.copied()
    }

    pub fn clear_actions_with_modifier(
        &self,
        modifier: InputModifier,
        input_state: &mut InputState,
    ) {
        for (_, v) in &self.mapping {
            for (action, modifier_set) in &v.actions {
                if modifier_set.is_set(modifier) {
                    input_state.set_down(*action, false);
                }
            }
        }
    }

    pub fn register_action(&mut self, code: KeyCode, action: InputAction) {
        self.register_modifier_action(code, InputModifierSet::empty(), action);
    }

    pub fn register_modifier_action(
        &mut self,
        code: KeyCode,
        modifiers: InputModifierSet,
        action: InputAction,
    ) {
        if let Some(key_set) = self.mapping.get_mut(&code) {
            key_set.insert(action, modifiers);
        } else {
            let mut key_set = KeyActionSet::new();

            key_set.insert(action, modifiers);

            self.mapping.insert(code, key_set);
        }
    }
}

pub fn is_input_keycode(code: KeyCode) -> bool {
    match code {
        KeyCode::Backquote => true,
        KeyCode::Backslash => true,
        KeyCode::BracketLeft => true,
        KeyCode::BracketRight => true,
        KeyCode::Comma => true,
        KeyCode::Digit0 => true,
        KeyCode::Digit1 => true,
        KeyCode::Digit2 => true,
        KeyCode::Digit3 => true,
        KeyCode::Digit4 => true,
        KeyCode::Digit5 => true,
        KeyCode::Digit6 => true,
        KeyCode::Digit7 => true,
        KeyCode::Digit8 => true,
        KeyCode::Digit9 => true,
        KeyCode::Equal => true,
        KeyCode::IntlBackslash => true,
        KeyCode::KeyA => true,
        KeyCode::KeyB => true,
        KeyCode::KeyC => true,
        KeyCode::KeyD => true,
        KeyCode::KeyE => true,
        KeyCode::KeyF => true,
        KeyCode::KeyG => true,
        KeyCode::KeyH => true,
        KeyCode::KeyI => true,
        KeyCode::KeyJ => true,
        KeyCode::KeyK => true,
        KeyCode::KeyL => true,
        KeyCode::KeyM => true,
        KeyCode::KeyN => true,
        KeyCode::KeyO => true,
        KeyCode::KeyP => true,
        KeyCode::KeyQ => true,
        KeyCode::KeyR => true,
        KeyCode::KeyS => true,
        KeyCode::KeyT => true,
        KeyCode::KeyU => true,
        KeyCode::KeyV => true,
        KeyCode::KeyW => true,
        KeyCode::KeyX => true,
        KeyCode::KeyY => true,
        KeyCode::KeyZ => true,
        KeyCode::Minus => true,
        KeyCode::Period => true,
        KeyCode::Quote => true,
        KeyCode::Semicolon => true,
        KeyCode::Slash => true,
        KeyCode::Enter => true,
        KeyCode::Space => true,
        KeyCode::Numpad0 => true,
        KeyCode::Numpad1 => true,
        KeyCode::Numpad2 => true,
        KeyCode::Numpad3 => true,
        KeyCode::Numpad4 => true,
        KeyCode::Numpad5 => true,
        KeyCode::Numpad6 => true,
        KeyCode::Numpad7 => true,
        KeyCode::Numpad8 => true,
        KeyCode::Numpad9 => true,
        KeyCode::NumpadBackspace => true,
        KeyCode::NumpadComma => true,
        KeyCode::NumpadDivide => true,
        KeyCode::NumpadEnter => true,
        KeyCode::NumpadEqual => true,
        KeyCode::NumpadHash => true,
        KeyCode::NumpadParenLeft => true,
        KeyCode::NumpadParenRight => true,
        KeyCode::NumpadStar => true,
        KeyCode::NumpadSubtract => true,
        _ => false,
    }
}
