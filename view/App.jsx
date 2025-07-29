import React from "react";
import {ThemeProvider, createTheme} from "@material-ui/core/styles";
import { createBrowserRouter, Outlet, RouterProvider, Link } from 'react-router-dom'
import {TasksList} from "./TasksList";
import {TaskDetails} from "./TaskDetails";
import {TaskStatistics} from "./TaskStatistics";
import {Typography} from "@material-ui/core";

const theme = createTheme({
    typography: {
        fontFamily: [
            "-apple-system",
            "BlinkMacSystemFont",
            "Segoe UI",
            "Roboto",
            "Oxygen",
            "Ubuntu",
            "Cantarell",
            "Fira Sans",
            "Droid Sans",
            "Helvetica Neue",
            "sans-serif"
        ].join(","),
    },
    zoom: 10,
    palette: {
        primary: {
            main: "#053f5e"
        },
        secondary: {
            main: "#5cb85c"
        },
        background: {
            default: "#f7f9fc"
        }
    },
});

const Navbar = () => {
    return (
        <nav>
            <ul>
                <li><Link to="/"><Typography >Tasks</Typography></Link></li>
                <li><Link to="/statistics"><Typography>Statistics</Typography></Link></li>
            </ul>
        </nav>
    )
}

const router = createBrowserRouter([{
    path: '',
    element: <div><Navbar/><Outlet/></div>,
    errorElement: <div>404 - Page not found</div>,
    children: [
        { path: '', element: <TasksList/> },
        { path: '/details', element: <TaskDetails/> },
        { path: '/statistics', element: <TaskStatistics/> },
    ],
}])

export function App () {
    return <ThemeProvider theme={theme}><RouterProvider router={router}/></ThemeProvider>
}