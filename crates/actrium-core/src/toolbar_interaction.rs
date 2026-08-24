use yasm::{define_state_machine, StateMachineInstance};

mod machine {
    use super::define_state_machine;

    define_state_machine! {
        name: ToolbarTabInteractionMachine,
        states: {
            Inactive,
            Active,
            AddressEditing,
            DraggingInactive,
            DraggingActive
        },
        inputs: {
            Activate,
            Deactivate,
            BeginAddressEditing,
            EndAddressEditing,
            BeginDrag,
            FinishDragInactive,
            FinishDragActive,
            Cancel
        },
        initial: Inactive,
        transitions: {
            Inactive + Activate => Active,
            Inactive + Deactivate => Inactive,
            Inactive + BeginDrag => DraggingInactive,
            Inactive + Cancel => Inactive,

            Active + Activate => Active,
            Active + Deactivate => Inactive,
            Active + BeginAddressEditing => AddressEditing,
            Active + EndAddressEditing => Active,
            Active + BeginDrag => DraggingActive,
            Active + Cancel => Active,

            AddressEditing + Activate => AddressEditing,
            AddressEditing + Deactivate => Inactive,
            AddressEditing + BeginAddressEditing => AddressEditing,
            AddressEditing + EndAddressEditing => Active,
            AddressEditing + Cancel => Active,

            DraggingInactive + FinishDragInactive => Inactive,
            DraggingInactive + FinishDragActive => Active,
            DraggingInactive + Cancel => Inactive,

            DraggingActive + FinishDragInactive => Inactive,
            DraggingActive + FinishDragActive => Active,
            DraggingActive + Cancel => Active
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarTabInteractionPhase {
    Inactive,
    Active,
    AddressEditing,
    DraggingInactive,
    DraggingActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarTabInteractionInput {
    Activate,
    Deactivate,
    BeginAddressEditing,
    EndAddressEditing,
    BeginDrag,
    FinishDragInactive,
    FinishDragActive,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarTabInteractionTransitionInfo {
    pub phase: ToolbarTabInteractionPhase,
    pub accepted: bool,
    pub accepts_tab_buttons: bool,
    pub accepts_address_input: bool,
    pub permits_tab_drag: bool,
    pub suppresses_window_drag: bool,
}

impl From<ToolbarTabInteractionPhase> for machine::State {
    fn from(value: ToolbarTabInteractionPhase) -> Self {
        match value {
            ToolbarTabInteractionPhase::Inactive => Self::Inactive,
            ToolbarTabInteractionPhase::Active => Self::Active,
            ToolbarTabInteractionPhase::AddressEditing => Self::AddressEditing,
            ToolbarTabInteractionPhase::DraggingInactive => Self::DraggingInactive,
            ToolbarTabInteractionPhase::DraggingActive => Self::DraggingActive,
        }
    }
}

impl From<machine::State> for ToolbarTabInteractionPhase {
    fn from(value: machine::State) -> Self {
        match value {
            machine::State::Inactive => Self::Inactive,
            machine::State::Active => Self::Active,
            machine::State::AddressEditing => Self::AddressEditing,
            machine::State::DraggingInactive => Self::DraggingInactive,
            machine::State::DraggingActive => Self::DraggingActive,
        }
    }
}

impl From<ToolbarTabInteractionInput> for machine::Input {
    fn from(value: ToolbarTabInteractionInput) -> Self {
        match value {
            ToolbarTabInteractionInput::Activate => Self::Activate,
            ToolbarTabInteractionInput::Deactivate => Self::Deactivate,
            ToolbarTabInteractionInput::BeginAddressEditing => Self::BeginAddressEditing,
            ToolbarTabInteractionInput::EndAddressEditing => Self::EndAddressEditing,
            ToolbarTabInteractionInput::BeginDrag => Self::BeginDrag,
            ToolbarTabInteractionInput::FinishDragInactive => Self::FinishDragInactive,
            ToolbarTabInteractionInput::FinishDragActive => Self::FinishDragActive,
            ToolbarTabInteractionInput::Cancel => Self::Cancel,
        }
    }
}

pub fn transition(
    phase: ToolbarTabInteractionPhase,
    input: ToolbarTabInteractionInput,
) -> ToolbarTabInteractionTransitionInfo {
    let mut machine =
        StateMachineInstance::<machine::ToolbarTabInteractionMachine>::from_state(phase.into());
    let accepted = machine.transition(input.into()).is_ok();
    transition_info(machine.current_state().clone().into(), accepted)
}

pub fn transition_info(
    phase: ToolbarTabInteractionPhase,
    accepted: bool,
) -> ToolbarTabInteractionTransitionInfo {
    ToolbarTabInteractionTransitionInfo {
        phase,
        accepted,
        accepts_tab_buttons: phase == ToolbarTabInteractionPhase::Active,
        accepts_address_input: phase == ToolbarTabInteractionPhase::AddressEditing,
        permits_tab_drag: phase != ToolbarTabInteractionPhase::AddressEditing,
        // The tab strip is interactive chrome in every phase. Window movement is
        // never a valid result of a pointer sequence that started inside it.
        suppresses_window_drag: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_editing_rejects_tab_drag() {
        let result = transition(
            ToolbarTabInteractionPhase::AddressEditing,
            ToolbarTabInteractionInput::BeginDrag,
        );

        assert!(!result.accepted);
        assert_eq!(result.phase, ToolbarTabInteractionPhase::AddressEditing);
        assert!(!result.permits_tab_drag);
        assert!(result.accepts_address_input);
    }

    #[test]
    fn active_and_inactive_tabs_enter_distinct_drag_phases() {
        let inactive = transition(
            ToolbarTabInteractionPhase::Inactive,
            ToolbarTabInteractionInput::BeginDrag,
        );
        let active = transition(
            ToolbarTabInteractionPhase::Active,
            ToolbarTabInteractionInput::BeginDrag,
        );

        assert_eq!(inactive.phase, ToolbarTabInteractionPhase::DraggingInactive);
        assert_eq!(active.phase, ToolbarTabInteractionPhase::DraggingActive);
        assert!(inactive.accepted);
        assert!(active.accepted);
    }

    #[test]
    fn every_phase_suppresses_window_drag() {
        for phase in [
            ToolbarTabInteractionPhase::Inactive,
            ToolbarTabInteractionPhase::Active,
            ToolbarTabInteractionPhase::AddressEditing,
            ToolbarTabInteractionPhase::DraggingInactive,
            ToolbarTabInteractionPhase::DraggingActive,
        ] {
            assert!(transition_info(phase, true).suppresses_window_drag);
        }
    }

    #[test]
    fn drag_is_exclusive_until_it_finishes_or_is_cancelled() {
        for phase in [
            ToolbarTabInteractionPhase::DraggingInactive,
            ToolbarTabInteractionPhase::DraggingActive,
        ] {
            for input in [
                ToolbarTabInteractionInput::BeginDrag,
                ToolbarTabInteractionInput::BeginAddressEditing,
                ToolbarTabInteractionInput::Activate,
                ToolbarTabInteractionInput::Deactivate,
            ] {
                let result = transition(phase, input);
                assert!(
                    !result.accepted,
                    "{phase:?} unexpectedly accepted {input:?}"
                );
                assert_eq!(result.phase, phase);
                assert!(result.suppresses_window_drag);
            }
        }
    }

    #[test]
    fn only_the_active_non_editing_phase_accepts_tab_buttons() {
        for phase in [
            ToolbarTabInteractionPhase::Inactive,
            ToolbarTabInteractionPhase::Active,
            ToolbarTabInteractionPhase::AddressEditing,
            ToolbarTabInteractionPhase::DraggingInactive,
            ToolbarTabInteractionPhase::DraggingActive,
        ] {
            assert_eq!(
                transition_info(phase, true).accepts_tab_buttons,
                phase == ToolbarTabInteractionPhase::Active
            );
        }
    }
}
