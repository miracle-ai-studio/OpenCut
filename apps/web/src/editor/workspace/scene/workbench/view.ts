import { createContext, useContext } from "react";

// Local panel navigation only; project state and commands stay in Session.
export const WorkbenchView = createContext({
  showHistory: () => {},
  showProperties: () => {},
});
export const useWorkbenchView = () => useContext(WorkbenchView);
