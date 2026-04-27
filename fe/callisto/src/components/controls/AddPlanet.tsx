import * as React from "react";
import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { POSITION_SCALE } from "lib/universal";
import { Accordion } from "lib/Accordion";
import { Tooltip } from "react-tooltip";
import { CiCircleQuestion } from "react-icons/ci";
import {
  Planet,
  defaultPlanet,
  findPlanet,
  PlanetVisualEffect,
} from "lib/entities";

import { addPlanet } from "lib/serverManager";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

type AddShipProps = unknown;

const DEFAULT_COLOR = "yellow";

export const AddPlanet: React.FC<AddShipProps> = () => {
  const entities = useAppSelector(entitiesSelector);
  const planetNameRef = useRef<HTMLInputElement>(null);

  const planetNames = useMemo(
    () => entities.planets.map((planet: Planet) => planet.name),
    [entities.ships],
  );
  const initialTemplate = useMemo(
    () => ({
      name: "Planet",
      xpos: "0",
      ypos: "0",
      zpos: "0",
      color: "yellow" as string | null,
      radius: 6371,
      mass: 5.972e24,
      primary: null as string | null,
      visual_effects: [] as PlanetVisualEffect[],
    }),
    [entities],
  );

  const [addPlanetData, setAddPlanetData] = useState(initialTemplate);

  useEffect(() => {
    const current =
      entities.planets.find((planet) => planet.name === addPlanetData.name) ||
      null;
    if (current != null) {
      const template = {
        name: current.name,
        xpos: (current.position[0] / POSITION_SCALE).toString(),
        ypos: (current.position[1] / POSITION_SCALE).toString(),
        zpos: (current.position[2] / POSITION_SCALE).toString(),
        radius: current.radius / 1000, // Convert from meters to km
        mass: current.mass,
        color: current.color,
        primary: current.primary,
        visual_effects: current.visual_effects,
      };
      setAddPlanetData(template);
    }
  }, [addPlanetData.name, entities.ships]);

  const handleChange = useMemo(
    () => (event: React.ChangeEvent<HTMLInputElement>) => {
      event.target.style.color = "black";
      if (event.target.name === "name") {
        if (planetNames.includes(event.target.value)) {
          event.target.style.color = "green";
          const planet = findPlanet(entities, event.target.value);
          if (planet != null) {
            setAddPlanetData({
              name: event.target.value,
              xpos: (planet.position[0] / POSITION_SCALE).toString(),
              ypos: (planet.position[1] / POSITION_SCALE).toString(),
              zpos: (planet.position[2] / POSITION_SCALE).toString(),
              mass: planet.mass,
              radius: planet.radius,
              color: planet.color,
              primary: planet.primary,
              visual_effects: planet.visual_effects,
            });
          }
        }
      }
      setAddPlanetData({
        ...addPlanetData,
        [event.target.name]: event.target.value,
      });
    },
    [planetNames, entities, setAddPlanetData, addPlanetData],
  );

  const handleSubmit = useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const position: [number, number, number] = [
        Number(addPlanetData.xpos) * POSITION_SCALE,
        Number(addPlanetData.ypos) * POSITION_SCALE,
        Number(addPlanetData.zpos) * POSITION_SCALE,
      ];
      const planet =
        findPlanet(entities, addPlanetData.name) || defaultPlanet();

      let color = addPlanetData.color;
      if (!color || color === "") {
        color = DEFAULT_COLOR;
      }

      const revision: Planet = {
        ...planet,
        name: addPlanetData.name,
        position,
        mass: addPlanetData.mass,
        radius: addPlanetData.radius * 1000, // Convert from km to meters
        velocity: [0, 0, 0],
        primary: addPlanetData.primary,
        color: color,
        visual_effects: addPlanetData.visual_effects,
      };

      addPlanet(revision);
      //setAddPlanetData(initialTemplate);
      //planetNameRef.current!.style.color = "black";
    },
    [addPlanetData, entities, initialTemplate, planetNameRef],
  );

  const handleColorChange = useCallback(
    (color: string | null) =>
      setAddPlanetData({
        ...addPlanetData,
        color: color || "",
      }),
    [addPlanetData, setAddPlanetData],
  );

  const handlePrimaryChange = useCallback(
    (primary: string) => {
      setAddPlanetData({
        ...addPlanetData,
        primary: primary === "" ? null : primary,
      });
    },
    [addPlanetData, setAddPlanetData],
  );

  const updateOrAddLabel = useMemo(
    () => (planetNames.includes(addPlanetData.name) ? "Update" : "Add"),
    [addPlanetData.name, planetNames],
  );

  return (
    <Accordion id="add-planet-header" title="Add Planet" initialOpen={false}>
      <form id="add-planet" className="control-form" onSubmit={handleSubmit}>
        <div id="add-planet-top-part">
          <label className="control-label">
            Name
            <input
              id="add-ship-name-input"
              className="control-name-input control-input"
              name="name"
              type="text"
              onChange={handleChange}
              value={addPlanetData.name}
              ref={planetNameRef}
            />
          </label>
          <label className="control-label">
            Position (km)
            <div className="coordinate-input">
              <input
                className="control-input"
                name="xpos"
                type="text"
                value={addPlanetData.xpos}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="ypos"
                type="text"
                value={addPlanetData.ypos}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="zpos"
                type="text"
                value={addPlanetData.zpos}
                onChange={handleChange}
              />
            </div>
          </label>
          <div className="coordinate-input">
            <label className="control-label">
              Mass (kg)
              <input
                className="mass-input control-input"
                name="mass"
                type="text"
                value={addPlanetData.mass}
                onChange={handleChange}
              />
            </label>
            <label className="control-label">
              Radius (km)
              <input
                className="mass-input control-input"
                name="radius"
                type="text"
                value={addPlanetData.radius}
                onChange={handleChange}
              />
            </label>
          </div>
          <div className="coordinate-input">
            <PlanetList
              planetName={addPlanetData.primary}
              setPlanetName={handlePrimaryChange}
              planetNames={planetNames}
              exclude={addPlanetData.name}
            />
            <ColorChooser
              color={addPlanetData.color}
              setColor={handleColorChange}
            />
          </div>
          <PlanetEffectChooser
            effects={addPlanetData.visual_effects}
            setEffects={(effects) =>
              setAddPlanetData({ ...addPlanetData, visual_effects: effects })
            }
          />
          <input
            className="control-input control-button blue-button"
            type="submit"
            value={updateOrAddLabel}
          />
        </div>
      </form>
    </Accordion>
  );
};

function PlanetList(args: {
  planetName: string | null;
  setPlanetName: (planet: string) => void;
  planetNames: string[];
  exclude: string;
}) {
  const selectRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (selectRef.current != null) {
      selectRef.current.value = args.planetName || "";
    }
  }, [args.planetName]);

  const handlePlanetListSelectChange = useCallback(
    (event: React.ChangeEvent<HTMLSelectElement>) => {
      const value = event.target.value;
      args.setPlanetName(value);
    },
    [args],
  );

  return (
    <div className="planet-list">
      <label className="control-label">
        Primary
        <select
          className="select-dropdown control-name-input control-input"
          name="planet_list_choice"
          ref={selectRef}
          defaultValue={args.planetName || ""}
          onChange={handlePlanetListSelectChange}
        >
          {[
            <option key="null-planet-list" value="">
              {"" as String}
            </option>,
          ].concat(
            Object.values(args.planetNames)
              .sort((a, b) => a.localeCompare(b))
              .filter((planet) => planet !== args.exclude)
              .map((planet) => (
                <option
                  key={planet + "-planet_list"}
                  value={planet}
                >{`${planet}`}</option>
              )),
          )}
        </select>
      </label>
    </div>
  );
}

function ColorChooser(args: {
  color: string | null;
  setColor: (color: string | null) => void;
}) {
  const colorRef = useRef<HTMLInputElement>(null);

  const handleChange = () => {
    args.setColor(colorRef.current?.value ?? null);
  };
  return (
    <label className="control-label">
      Color
      <input
        className="mass-input control-input"
        name="color"
        type="text"
        value={args.color ?? ""}
        onChange={handleChange}
        ref={colorRef}
      />
    </label>
  );
}

function PlanetEffectChooser(args: {
  effects: PlanetVisualEffect[];
  setEffects: (effects: PlanetVisualEffect[]) => void;
}) {
  const surfaceEffects = [
    PlanetVisualEffect.CONTINENTS,
    PlanetVisualEffect.STRIPED_BANDS,
    PlanetVisualEffect.LATITUDE_COLOR,
    PlanetVisualEffect.NOISE_TEXTURE,
    PlanetVisualEffect.PHONG_LIGHTING,
  ];

  const currentSurfaceEffect =
    surfaceEffects.find((effect) => args.effects.includes(effect)) ?? "";

  const updateEffects = (
    surfaceEffect: PlanetVisualEffect | "",
    layerableEffects: PlanetVisualEffect[],
  ) => {
    args.setEffects(
      surfaceEffect === ""
        ? layerableEffects
        : [surfaceEffect, ...layerableEffects],
    );
  };

  const handleSurfaceChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedEffect = event.target.value as PlanetVisualEffect | "";
    const layerableEffects = args.effects.filter(
      (effect) => !surfaceEffects.includes(effect),
    );
    updateEffects(selectedEffect, layerableEffects);
  };

  const handleLayerableChange = (effect: PlanetVisualEffect) => {
    const surfaceEffect = currentSurfaceEffect;
    const layerableEffects = args.effects.filter(
      (currentEffect) => !surfaceEffects.includes(currentEffect),
    );
    const nextLayerableEffects = layerableEffects.includes(effect)
      ? layerableEffects.filter((currentEffect) => currentEffect !== effect)
      : [...layerableEffects, effect];

    updateEffects(surfaceEffect, nextLayerableEffects);
  };

  return (
    <>
      <div className="control-label">
        Effects:
        <table className="planet-effect-table">
          <tbody>
            <tr>
              <td colSpan={2}>
                <label className="control-label">
                  Surface Effect
                  <select
                    className="control-input planet-effect-select"
                    value={currentSurfaceEffect}
                    onChange={handleSurfaceChange}
                  >
                    <option value="">None</option>
                    <option value={PlanetVisualEffect.CONTINENTS}>
                      Continents
                    </option>
                    <option value={PlanetVisualEffect.STRIPED_BANDS}>
                      Striped Bands
                    </option>
                    <option value={PlanetVisualEffect.LATITUDE_COLOR}>
                      Latitude Color
                    </option>
                    <option value={PlanetVisualEffect.NOISE_TEXTURE}>
                      Noise Texture
                    </option>
                    <option value={PlanetVisualEffect.PHONG_LIGHTING}>
                      Phong Lighting
                    </option>
                  </select>
                </label>
              </td>
            </tr>
            <tr>
              <td>
                <label className="control-label">
                  Animated Clouds
                  <input
                    className="planet-effect-checkbox"
                    type="checkbox"
                    checked={args.effects.includes(
                      PlanetVisualEffect.ANIMATED_CLOUDS,
                    )}
                    onChange={() =>
                      handleLayerableChange(PlanetVisualEffect.ANIMATED_CLOUDS)
                    }
                  />
                </label>
              </td>
              <td>
                <label className="control-label">
                  Atmosphere Ring
                  <input
                    className="planet-effect-checkbox"
                    type="checkbox"
                    checked={args.effects.includes(
                      PlanetVisualEffect.ATMOSPHERE_RING,
                    )}
                    onChange={() =>
                      handleLayerableChange(PlanetVisualEffect.ATMOSPHERE_RING)
                    }
                  />
                </label>
              </td>
            </tr>
            <tr>
              <td>
                <label className="control-label">
                  Planetary Ring
                  <input
                    className="planet-effect-checkbox"
                    type="checkbox"
                    checked={args.effects.includes(
                      PlanetVisualEffect.PLANETARY_RING,
                    )}
                    onChange={() =>
                      handleLayerableChange(PlanetVisualEffect.PLANETARY_RING)
                    }
                  />
                </label>
              </td>
              <td>&nbsp;</td>
            </tr>
          </tbody>
        </table>
      </div>
    </>
  );
}
