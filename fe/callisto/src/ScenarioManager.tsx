import React from "react";
import {joinScenario, createScenario } from "./ServerManager";


// This function will generate a three word name (with hyphens between the words) or random words
// to be used as the name for a new scenario.
function generateUniqueHyphenatedName() {
    // eslint-disable-next-line array-element-newline
    const words = [
        "ocean", "mountain", "river", "forest", "desert", "island", "valley", "canyon",
        "meadow", "glacier", "sunset", "sunrise", "twilight", "midnight", "shadow", "light",
        "storm", "breeze", "whisper", "silence", "echo", "journey", "voyage", "adventure",
        "discovery", "secret", "mystery", "wonder", "magic", "dream", "fantasy", "reality",
        "truth", "wisdom", "courage", "strength", "peace", "joy", "laughter", "tears",
        "heart", "soul", "spirit", "mind", "thought", "idea", "vision", "passion",
        "grace", "beauty", "charm", "elegance", "style", "fashion", "art", "music",
        "dance", "theater", "cinema", "story", "poem", "novel", "legend", "myth",
        "destiny", "fate", "chance", "luck", "hope", "faith", "charity", "kindness",
        "friendship", "love", "family", "home", "garden", "flower", "tree", "bird",
        "fish", "star", "planet", "galaxy", "universe", "time", "space", "matter",
        "energy", "motion", "change", "growth", "creation", "destruction", "balance",
        "harmony", "silence", "sound", "color", "shape", "texture", "taste", "smell",
        "touch", "feeling", "emotion", "desire", "belief", "knowledge", "learning"
        ];
    const selectedWords = [];
    const availableIndices = [...words.keys()];

    while (selectedWords.length < 3 && availableIndices.length > 0) {
        const randomIndex = Math.floor(Math.random() * availableIndices.length);
        const wordIndex = availableIndices.splice(randomIndex, 1)[0];
        selectedWords.push(words[wordIndex]);
    }

    return selectedWords.join('-');
}

type ScenarioManagerProps = {
    scenarios: string[];
};

export const ScenarioManager: React.FC<ScenarioManagerProps> = ({scenarios}) => {
    const [scenario, setScenario] = React.useState<string | null>(null);

    const scenarioName = generateUniqueHyphenatedName();

    function handleScenarioSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
        setScenario(event.target.value);
    }

    function handleJoinScenario(event: React.FormEvent<HTMLFormElement>) {
        event.preventDefault();

        const scenarioName = (event.currentTarget.elements[0] as HTMLInputElement).value;

        joinScenario(scenarioName);
        console.log("Joining scenario: " + scenarioName);
    }

    function handleCreateScenario(event: React.FormEvent<HTMLFormElement>) {
        event.preventDefault();
        createScenario(scenarioName, scenario?? "");

        console.log(`Creating scenario ${scenarioName}: ${scenario?? ""}`);
    }



    return (
        <div className="scenario-manager">

            <form className="scenario-join-form" onSubmit={handleJoinScenario}>
                <h1>Join Existing Scenario</h1>
                <br />
                <input id="scenario-to-join" className= "control-name-input control-input" type="text" />
                <button className="control-input control-button blue-button" type="submit">Join</button>
            </form>

            <form className = "scenario-create-form" onSubmit={handleCreateScenario}>
                <h1>Create New Scenario</h1>
                <br />
                <span className = "label-scenario-name"><b>Name:</b> {scenarioName}</span>
                    <select className="select-dropdown control-name-input control-input" name="scenario_selector" value={scenario?? ""} onChange={handleScenarioSelectChange}>
                        {scenarios.map((scenario) => (
                            <option key={scenario} value={scenario}>
                                {scenario}
                            </option>
                        ))}
                    </select>
                    <button className="control-input control-button blue-button" type="submit">Create</button>
            </form>
        </div>);
};
