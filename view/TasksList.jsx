import { Table, TableBody, TableCell, TableHead, TableRow } from "@material-ui/core";
import {useState} from "react";

export const TasksList = () => {
    const [tasks, setTasks] = useState([]); // useState hook for storing the tasks

    return (
        <Table>
            <TableHead>
                <TableRow>
                    <TableCell>Title</TableCell>
                    <TableCell>Subtitle</TableCell>
                    <TableCell>Category</TableCell>
                    <TableCell>Status</TableCell>
                    <TableCell>Due Date</TableCell>
                </TableRow>
            </TableHead>
            <TableBody>
                {tasks.map((task) => (
                    <TableRow key={task.id}>
                        <TableCell>{task.title}</TableCell>
                        <TableCell>{task.subtitle}</TableCell>
                        <TableCell>{task.category}</TableCell>
                        <TableCell>{task.status}</TableCell>
                        <TableCell>{task.dueDate}</TableCell>
                    </TableRow>
                ))}
            </TableBody>
        </Table>
    );
};
