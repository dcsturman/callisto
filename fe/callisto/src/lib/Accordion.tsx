import * as React from "react";
import { useState, useMemo, ReactNode } from "react";
import { FaChevronDown, FaChevronUp } from "react-icons/fa";

export function Accordion({ className, id, title, initialOpen, children } : { className?: string, id?: string, title: string, initialOpen?: boolean, children?: ReactNode }) {
  const [isOpen, setIsOpen] = useState(initialOpen === undefined || initialOpen);

  const toggleHandler = useMemo(() => () => {
    setIsOpen(!isOpen);
  }, [isOpen]);

  const chevron = useMemo(() => isOpen ? <FaChevronUp /> : <FaChevronDown />, [isOpen]);

  return (
    <div className={"accordion " + className} id={id}>
      <span className="accordion-header" onClick={toggleHandler}>{title}{chevron}</span>
      {isOpen && (
        <div className={"accordion-content " + className+"-header"}>{children}</div>
      )}
    </div>
  );
}
export default Accordion;