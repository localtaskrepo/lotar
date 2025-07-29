import { useTranslation } from 'react-i18next';
import { ExpansionPanel, ExpansionPanelSummary, ExpansionPanelDetails, TextField, Typography } from "@material-ui/core";

export const TaskDetails = ({ match }) => {
    const taskId = match.params.id;
    const { t } = useTranslation();
    const [task, setTask] = useState({}); // useState hook for storing the current task

    return (
        <ExpansionPanel>
            <ExpansionPanelSummary>
                <Typography>{task.title}</Typography>
            </ExpansionPanelSummary>
            <ExpansionPanelDetails>
                <form>
                    <TextField label={t('subtitle')} value={task.subtitle} onChange={(e) => setTask({ ...task, subtitle: e.target.value })} />
                    <TextField label={t('description')} value={task.description} onChange={(e) => setTask({ ...task, description: e.target.value })} />
                    <TextField label={t('category')} value={task.category} onChange={(e) => setTask({ ...task, category: e.target.value })} />
                    <TextField label={t('status')} value={task.status} onChange={(e) => setTask({ ...task, status: e.target.value })} />
                    <TextField label={t('dueDate')} value={task.dueDate} onChange={(e) => setTask({ ...task, dueDate: e.target.value })} />
                    <TextField label={t('createDate')} value={task.createDate} onChange={(e) => setTask({ ...task, createDate: e.target.value })} />
                    <TextField label={t('project')} value={task.project} onChange={(e) => setTask({ ...task, project: e.target.value })} />
                    <TextField label={t('tags')} value={task.tags} onChange={(e) => setTask({ ...task, tags: e.target.value })} />
                </form>
            </ExpansionPanelDetails>
        </ExpansionPanel>
    );
};
