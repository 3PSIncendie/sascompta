import type {
  Employee,
  EmploymentContract,
  LeaveBalance,
  LeaveRequest,
  Payslip,
  SocialRateTable,
} from "../../lib/types";
import { computePayslipAmounts } from "./format";

export function previewEmployees(): Employee[] {
  return [
    {
      id: 1,
      code: "E001",
      firstName: "Camille",
      lastName: "Martin",
      birthDate: "1992-04-12",
      address: "5 rue des Ateliers\n69003 Lyon",
      email: "camille.martin@demo.sas",
      ibanRef: "FR76****001",
      active: true,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
    {
      id: 2,
      code: "E002",
      firstName: "Noah",
      lastName: "Bernard",
      birthDate: "2004-11-03",
      email: "noah.bernard@demo.sas",
      active: true,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
  ];
}

export function previewContracts(): EmploymentContract[] {
  return [
    {
      id: 1,
      employeeId: 1,
      employeeCode: "E001",
      employeeName: "Camille Martin",
      kind: "cdi",
      startDate: "2025-09-01",
      endDate: null,
      jobTitle: "Technicien détection",
      weeklyHoursBp: 3500,
      grossMonthlyCents: 280_000,
      conventionRef: "Syntec",
      status: "active",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
    {
      id: 2,
      employeeId: 2,
      employeeCode: "E002",
      employeeName: "Noah Bernard",
      kind: "apprentice",
      startDate: "2025-09-01",
      endDate: "2027-08-31",
      jobTitle: "Apprenti installateur",
      weeklyHoursBp: 3500,
      grossMonthlyCents: 95_000,
      status: "active",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
  ];
}

export function previewLeaveBalances(): LeaveBalance[] {
  return [
    {
      employeeId: 1,
      employeeCode: "E001",
      employeeName: "Camille Martin",
      year: 2026,
      acquiredDaysBp: 2500,
      takenDaysBp: 500,
      remainingDaysBp: 2000,
    },
    {
      employeeId: 2,
      employeeCode: "E002",
      employeeName: "Noah Bernard",
      year: 2026,
      acquiredDaysBp: 2500,
      takenDaysBp: 0,
      remainingDaysBp: 2500,
    },
  ];
}

export function previewLeaveRequests(): LeaveRequest[] {
  return [
    {
      id: 1,
      employeeId: 1,
      employeeCode: "E001",
      employeeName: "Camille Martin",
      fromDate: "2026-07-20",
      toDate: "2026-07-24",
      daysBp: 500,
      status: "approved",
      notes: "Congés d’été",
      createdAt: "2026-03-01T00:00:00Z",
      updatedAt: "2026-03-02T00:00:00Z",
    },
  ];
}

export function previewSocialRates(): SocialRateTable[] {
  return [
    {
      id: 1,
      validFrom: "2026-01-01",
      payloadJson: JSON.stringify({
        employee_rate_bp: 2200,
        employer_rate_bp: 4200,
      }),
      label: "Barème agrégé MVP 2026",
      createdAt: "2026-01-01T00:00:00Z",
    },
  ];
}

export function previewPayslips(): Payslip[] {
  const amounts = computePayslipAmounts(280_000, 2200, 4200, 0);
  return [
    {
      id: 1,
      employeeId: 1,
      employeeCode: "E001",
      employeeName: "Camille Martin",
      contractId: 1,
      periodYm: "2026-03",
      status: "draft",
      grossCents: 280_000,
      employeeChargesCents: amounts.employeeChargesCents,
      employerChargesCents: amounts.employerChargesCents,
      netCents: amounts.netCents,
      pasRateBp: null,
      pasCents: 0,
      entryId: null,
      notes: null,
      createdAt: "2026-03-31T00:00:00Z",
      updatedAt: "2026-03-31T00:00:00Z",
      lines: [
        {
          id: 1,
          payslipId: 1,
          lineNo: 1,
          code: "BRUT",
          label: "Salaire brut",
          baseCents: 280_000,
          rateBp: 0,
          employeeCents: 280_000,
          employerCents: 0,
        },
        {
          id: 2,
          payslipId: 1,
          lineNo: 2,
          code: "COTIS_SAL",
          label: "Cotisations salariales (agrégées)",
          baseCents: 280_000,
          rateBp: 2200,
          employeeCents: -amounts.employeeChargesCents,
          employerCents: 0,
        },
        {
          id: 3,
          payslipId: 1,
          lineNo: 3,
          code: "COTIS_PAT",
          label: "Cotisations patronales (agrégées)",
          baseCents: 280_000,
          rateBp: 4200,
          employeeCents: 0,
          employerCents: amounts.employerChargesCents,
        },
        {
          id: 4,
          payslipId: 1,
          lineNo: 5,
          code: "NET",
          label: "Net à payer",
          baseCents: amounts.netCents,
          rateBp: 0,
          employeeCents: amounts.netCents,
          employerCents: 0,
        },
      ],
    },
  ];
}
