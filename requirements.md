# Midi-Man Mk3

## overview
this is the third iteration of a project I want to start and I'd like to devise a plan for it before we do - I've had an idea for a midi controller for many years now, but i haven't had the time to actually implment it. The main idea is that there would be a row of 16 knobs (all rotary encoders, henceforth reffered to as knob for short), then 16 leds, then 16 buttons. think classic 808 interface, just 2 straight lines of knobs, lights, and buttons.

Each knob controls the note set for the step (based on the selected key) in the sequence, and the button itself controls if the step plays each pass.

then to the side would be a bank of 12 more buttons (4x3 grid) and 2 knobs, 1 knob is always dedicated to the master clock speed, the buttons selects the parameter the 2nd extra knob modulates the parameter.

The whole idea sort of mimics the "groove-boxes" of the late 90s early 2000s (the yamaha RS series and Roland MC series) where we have direct control over the initial pattern and then modifiers can be applied per step via a random chance from 0-100%, where 0 means the pattern always plays as input, and 100% means entirely random. 

## Controls
## regular mode
### knobs
- When playhead is stopped, turning knob right changes to the next note in the selected key/mode, turning to the left selects the previous note, no confirmation tap is needed. note is played when changed.
- when playhead is running - turning knob right changes to the next note in the selected key/mode, turning to the left selects the previous note. note is not played when changed, tapping the knob confirms the change instantly, otherwise change will be picked up next time playhead hits step 0.

### note buttons
- always enable/disable if notes should be played in sequence (this should be considered a musical "rest" when note is disabled)
- when note is enabled, led is lit, when it is disabled, led is off.

### tempo knob
- increases/decreases tempo.

### parameter buttons
Note: all params are unset until the knob is pressed to confirm.
1. Key (musical key i.e. AbCdEfG/sharp/flat)
2. Mode (musical mode i.e. Major/Minor/Dorian/Lacrian)
3. Swing (-/+)
4. Step (per note i.e. 1/4 notes, 1/8, 1/16)
5. - tbd -
6. Note Randomness (0-100) - chance each step note modifiers apply
7. Tempo Randomness (0-100) - chance tempo drifts (depends on setting in shift mode, can be per step/sequence).
8. Step Randomness (0-100) - chance each step step modifiers apply
9. loop (in/out/clear) - loops the playhead between the steps selected 
10. shift key - enters shift mode
11. pause (on/off) - pauses the playhead on the current step, resumes from current step when restarted 
12. stop/start - stops/starts playback. if pressed when paused, playhead is reset, but not restarted

## shift mode 
pressing the shift key changes the controls to respond like so:
### knobs
- When playhead is stopped - left/right increases/decreases notes default velocity value (0-100) - Tap Confirms
- when playhead is running - left/right increases/decreases notes default velocity value (0-100) - Tap Confirms

### note buttons
- always enable/disable if notes should be played in sequence (this should be considered a musical "rest" when note is disabled)
- when note is enabled, led is lit, when it is disabled, led is off.

### tempo knob
- increases/decreases tempo.

### parameter keys
Note: all params are unset until the knob is pressed to confirm.

1. Scale Quantization (enable/disable) - if enabled, note knobs select notes only in the current key/mode
2. Generate Random Sequence ()
3. Note Modifier - (off/0-12semitones/1-8 oct) - all to the left is off, then select between +-1-12 semi-tones, then beyond that we shorten it to 1-8 octaves, this is where a randomly selected note can land when substituted for the original note. Note - Scale Quanitization Applies. Default is +-1 octave
4. Skip Modifier - (off/on) - off = all enabled notes will play every sequence , on = random chance note will not play based on step randomness chance.
5. Velocity Modifier - (off/1-100) - off - note velocity always plays as set/ 0 - 100 - offset -+ applied if via note randomness roll.
6. Tempo Randomness Roll Point (off/step/beat/seq) - sets when tempo randomization should roll - never, once per step, once per beat (every 4 steps), every sequence (on step 0)
7. Tempo Variance Maxmimum amount - (1 - 99) normalized based on current tempo 1 means increase/decrease by 1% when applied, 50% increase up or down, etc. 
8. Tempo Randomness Type (random/up/down/breathe/pingpong) - how tempo randomness should be applied when triggered
- Random - Rolls for up or down, then rolls percentage amount to apply based on variance max.
- Up - tempo rolls Always increase tempo, rolls percent amount based on variance max
- down - temp rolls always decrease tempo, rolls percent amount based on variance max
- pingpong - tempo alternates between up and down rolls. 
- breathe - complex. tempo rolls a variance amount based on variance max. Then the next 6 rolls that indicate a tempo change use the same variance amount applied in sequence [up, up, up, down, down, down]
- note - future iterations should allow users to design their own tempo modification sequences, not for MVP though. 
9. - tbd - 
10. shift key - exits shift mode
11. pause (on/off) - pauses the playhead on the current step, resumes from current step when restarted 
12. stop/start - stops/starts playback. if pressed when paused, playhead is reset, but not restarted

## UI
given the complexity around the control surface having quite a few settings - i think a full UI indicating showing everything at once on screen. 

## Implementation.
i'm thinking the best approach is a 2 part implmentation
1. Engine
2. control surface.

## Engine
basically we build the entire thing out on PC, and put the controls on screen. I don't have a _specific_ development stack candidate in mind and this is just for me so IDC if we can make it into a profitable business or w/e so I think probably just targeting rasbian/fedora and telling everyone else they can port it to whatever they feel like is going to my answer. My primary concerns are speed, response, and memory usage over being pretty, so keep that in mind when researching a development stack (that means i guess, we lean toward something compiled ahead of time with a thin UI layer? best possible world is implementing it fast enough to run on something cheap like a pi-zero)

## Control surface
Given the sheer number of inputs we're probably going to need to multiplex it _somehow_ but the good news is that other than reading state/updating it's internal state, this isn't doing much so I'm thinking we can maybe do something USB HID compliant and then use a Raspberry Pi-Pico? I'm definitely open to ideas on this one.